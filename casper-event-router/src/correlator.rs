use anyhow::{Context, Result};
use casper_common::{EnrichedTransaction, RawEvent, TRANSACTION_ACCEPTED, TRANSACTION_PROCESSED};
use casper_types::{execution::ExecutionResult, Transaction, TransactionHash};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use std::time::Duration;

#[derive(Debug, Clone)]
struct AcceptedEvent {
    tx_hash: String,
    sender: String,
    accepted_at: DateTime<Utc>,
    raw_accepted: serde_json::Value,
}

#[derive(Debug, Clone)]
struct ProcessedEvent {
    tx_hash: String,
    status: String,
    processed_at: DateTime<Utc>,
    raw_processed: serde_json::Value,
}

/// Correlates TransactionAccepted and TransactionProcessed events by tx_hash
pub struct TransactionCorrelator {
    pending_accepted: DashMap<String, AcceptedEvent>,
    pending_processed: DashMap<String, ProcessedEvent>,
    #[allow(dead_code)]
    timeout: Duration,
}

impl TransactionCorrelator {
    pub fn new(timeout: Duration) -> Self {
        Self {
            pending_accepted: DashMap::new(),
            pending_processed: DashMap::new(),
            timeout,
        }
    }

    /// Attempt to correlate a raw event
    ///
    /// Returns Some(EnrichedTransaction) if both accepted and processed events are available
    /// Returns None if we're still waiting for the matching event
    pub fn correlate(&self, event: &RawEvent) -> Result<Option<EnrichedTransaction>> {
        match event.event_type.as_str() {
            TRANSACTION_ACCEPTED => self.handle_accepted(event),
            TRANSACTION_PROCESSED => self.handle_processed(event),
            _ => Ok(None),
        }
    }

    fn handle_accepted(&self, event: &RawEvent) -> Result<Option<EnrichedTransaction>> {
        // Parse the TransactionAccepted event
        let transaction = serde_json::from_value::<Transaction>(
            event.payload["TransactionAccepted"].clone(),
        )
        .context("Failed to parse Transaction from TransactionAccepted event")?;

        let tx_hash = transaction.hash().to_hex_string();
        let sender = transaction.initiator_addr().account_hash().to_hex_string();

        let accepted_event = AcceptedEvent {
            tx_hash: tx_hash.clone(),
            sender,
            accepted_at: event.received_at,
            raw_accepted: event.payload.clone(),
        };

        // Check if we already have the ProcessedEvent
        if let Some((_, processed)) = self.pending_processed.remove(&tx_hash) {
            // We have both! Merge and return
            Ok(Some(Self::merge(accepted_event, processed)))
        } else {
            // Store the AcceptedEvent and wait for ProcessedEvent
            self.pending_accepted.insert(tx_hash.clone(), accepted_event);
            tracing::debug!("Stored pending accepted event for tx_hash: {}", tx_hash);
            Ok(None)
        }
    }

    fn handle_processed(&self, event: &RawEvent) -> Result<Option<EnrichedTransaction>> {
        // Parse the TransactionProcessed event
        let data = &event.payload["TransactionProcessed"];
        let execution_result = serde_json::from_value::<ExecutionResult>(
            data["execution_result"].clone(),
        )
        .context("Failed to parse ExecutionResult from TransactionProcessed event")?;

        let tx_hash = serde_json::from_value::<TransactionHash>(
            data["transaction_hash"].clone(),
        )
        .context("Failed to parse TransactionHash from TransactionProcessed event")?
        .to_hex_string();

        let timestamp = data["timestamp"]
            .as_str()
            .context("Missing timestamp in TransactionProcessed event")?;
        let processed_at = timestamp
            .parse::<DateTime<Utc>>()
            .context("Failed to parse timestamp")?;

        let status = match execution_result.error_message() {
            Some(_) => "failure".to_string(),
            None => "success".to_string(),
        };

        let processed_event = ProcessedEvent {
            tx_hash: tx_hash.clone(),
            status,
            processed_at,
            raw_processed: event.payload.clone(),
        };

        // Check if we already have the AcceptedEvent
        if let Some((_, accepted)) = self.pending_accepted.remove(&tx_hash) {
            // We have both! Merge and return
            Ok(Some(Self::merge(accepted, processed_event)))
        } else {
            // Store the ProcessedEvent and wait for AcceptedEvent
            self.pending_processed.insert(tx_hash.clone(), processed_event);
            tracing::debug!("Stored pending processed event for tx_hash: {}", tx_hash);
            Ok(None)
        }
    }

    fn merge(accepted: AcceptedEvent, processed: ProcessedEvent) -> EnrichedTransaction {
        EnrichedTransaction {
            tx_hash: accepted.tx_hash,
            accepted_at: accepted.accepted_at,
            processed_at: processed.processed_at,
            sender: accepted.sender,
            status: processed.status,
            raw_accepted: accepted.raw_accepted,
            raw_processed: processed.raw_processed,
        }
    }

    /// Clean up expired pending events (those older than timeout)
    ///
    /// This should be called periodically in a background task
    pub fn cleanup_expired(&self, now: DateTime<Utc>) {
        let timeout_secs = self.timeout.as_secs() as i64;

        self.pending_accepted.retain(|_, event| {
            let age = now.signed_duration_since(event.accepted_at).num_seconds();
            if age > timeout_secs {
                tracing::warn!(
                    "Removing expired accepted event for tx_hash: {} (age: {}s)",
                    event.tx_hash,
                    age
                );
                false
            } else {
                true
            }
        });

        self.pending_processed.retain(|_, event| {
            let age = now.signed_duration_since(event.processed_at).num_seconds();
            if age > timeout_secs {
                tracing::warn!(
                    "Removing expired processed event for tx_hash: {} (age: {}s)",
                    event.tx_hash,
                    age
                );
                false
            } else {
                true
            }
        });
    }

    /// Get statistics about pending events
    pub fn stats(&self) -> CorrelatorStats {
        CorrelatorStats {
            pending_accepted: self.pending_accepted.len(),
            pending_processed: self.pending_processed.len(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CorrelatorStats {
    pub pending_accepted: usize,
    pub pending_processed: usize,
}
