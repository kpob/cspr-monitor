use anyhow::{Context, Result};
use casper_common::{EnrichedTransaction, RawEvent, TRANSACTION_ACCEPTED, TRANSACTION_PROCESSED};
use casper_types::{
    execution::ExecutionResult, ExecutableDeployItem, Transaction, TransactionEntryPoint,
    TransactionHash, TransactionInvocationTarget, TransactionTarget,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use std::time::Duration;

#[derive(Debug, Clone)]
struct AcceptedEvent {
    tx_hash: String,
    sender: String,
    accepted_at: DateTime<Utc>,
    raw_accepted: serde_json::Value,
    contract_hash: Option<String>,
    entry_point: Option<String>,
    args: Option<serde_json::Value>,
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

        let ta_value = &event.payload["TransactionAccepted"];
        let (contract_hash, entry_point, args) = if let Some(v1) = ta_value.get("Version1") {
            extract_from_v1(&v1["payload"])
        } else if let Some(deploy) = ta_value.get("Deploy") {
            extract_from_deploy(deploy)
        } else {
            (None, None, None)
        };

        let accepted_event = AcceptedEvent {
            tx_hash: tx_hash.clone(),
            sender,
            accepted_at: event.received_at,
            raw_accepted: event.payload.clone(),
            contract_hash,
            entry_point,
            args,
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
            contract_hash: accepted.contract_hash,
            entry_point: accepted.entry_point,
            args: accepted.args,
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

/// Extract contract hash, entry point, and args from a V1 transaction payload value.
///
/// `payload_value` is the JSON object at `TransactionAccepted.Version1.payload`.
fn extract_from_v1(payload_value: &serde_json::Value) -> (Option<String>, Option<String>, Option<serde_json::Value>) {
    let fields = payload_value.get("fields");

    // Deserialize the target field as a typed TransactionTarget and extract the hash.
    let contract_hash = fields
        .and_then(|f| f.get("target"))
        .and_then(|t| serde_json::from_value::<TransactionTarget>(t.clone()).ok())
        .and_then(|t| match t {
            TransactionTarget::Stored { id, .. } => match id {
                TransactionInvocationTarget::ByPackageHash { addr, .. } => {
                    Some(addr.iter().map(|b| format!("{b:02x}")).collect())
                }
                TransactionInvocationTarget::ByHash(h) => {
                    Some(h.iter().map(|b| format!("{b:02x}")).collect())
                }
                _ => None,
            },
            _ => None,
        });

    // The SSE stream represents entry points in two forms:
    //   - plain string for native variants: "Transfer", "AddBid", etc.
    //   - object for custom entry points:   {"Custom": "mint"}
    // We try serde_json deserialization first (handles both), then fall back to
    // TransactionEntryPoint::from() for the lowercase-string form used in tests.
    let entry_point = fields
        .and_then(|f| f.get("entry_point"))
        .and_then(|ep| {
            serde_json::from_value::<TransactionEntryPoint>(ep.clone())
                .ok()
                .or_else(|| ep.as_str().map(TransactionEntryPoint::from))
        })
        .map(|ep| match ep {
            TransactionEntryPoint::Custom(name) => name,
            other => other.to_string(),
        });

    // Args are in {"Named": [["name", {"parsed": ..., ...}], ...]} format.
    // Build a simple {name: parsed_value} object for easy consumption.
    let args = fields
        .and_then(|f| f.get("args"))
        .and_then(|a| a.get("Named"))
        .and_then(|named| named.as_array())
        .map(|arr| {
            let mut map = serde_json::Map::new();
            for item in arr {
                if let (Some(name), Some(parsed)) = (
                    item.get(0).and_then(|n| n.as_str()),
                    item.get(1).and_then(|v| v.get("parsed")),
                ) {
                    map.insert(name.to_string(), parsed.clone());
                }
            }
            serde_json::Value::Object(map)
        });

    (contract_hash, entry_point, args)
}

/// Extract contract hash, entry point, and args from a Deploy transaction value.
///
/// `deploy_value` is the JSON object at `TransactionAccepted.Deploy`.
fn extract_from_deploy(deploy_value: &serde_json::Value) -> (Option<String>, Option<String>, Option<serde_json::Value>) {
    let session = deploy_value
        .get("session")
        .and_then(|s| serde_json::from_value::<ExecutableDeployItem>(s.clone()).ok());

    let contract_hash = session.as_ref().and_then(|s| match s {
        ExecutableDeployItem::StoredContractByHash { hash, .. } => Some(format!("{hash}")),
        ExecutableDeployItem::StoredVersionedContractByHash { hash, .. } => {
            Some(format!("{hash}"))
        }
        _ => None,
    });

    let entry_point = session
        .as_ref()
        .map(|s| s.entry_point_name().to_string())
        .filter(|s| !s.is_empty());

    // Deploy args live at deploy_value["session"]["args"] as a list of [name, cl_value] pairs.
    let args = deploy_value
        .get("session")
        .and_then(|s| s.get("args"))
        .and_then(|a| a.as_array())
        .map(|arr| {
            let mut map = serde_json::Map::new();
            for item in arr {
                if let (Some(name), Some(parsed)) = (
                    item.get(0).and_then(|n| n.as_str()),
                    item.get(1).and_then(|v| v.get("parsed")),
                ) {
                    map.insert(name.to_string(), parsed.clone());
                }
            }
            serde_json::Value::Object(map)
        });

    (contract_hash, entry_point, args)
}

#[derive(Debug, Clone)]
pub struct CorrelatorStats {
    pub pending_accepted: usize,
    pub pending_processed: usize,
}
