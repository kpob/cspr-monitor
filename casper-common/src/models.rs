use anyhow::Context;
use serde::{Deserialize, Serialize};
use sqlx::types::chrono::{DateTime, Utc};

use crate::KafkaMessage;

// Event type constants
pub const TRANSACTION_ACCEPTED: &str = "TransactionAccepted";
pub const TRANSACTION_PROCESSED: &str = "TransactionProcessed";
pub const BLOCK_ADDED: &str = "BlockAdded";

/// Raw event from the blockchain
#[derive(sqlx::FromRow, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawEvent {
    pub id: i64,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub received_at: DateTime<Utc>,
}

impl TryFrom<KafkaMessage> for RawEvent {
    type Error = anyhow::Error;

    fn try_from(message: KafkaMessage) -> Result<Self, Self::Error> {
        let payload = serde_json::from_str::<serde_json::Value>(&message.payload)
            .context("Failed to parse Kafka message payload as JSON")?;
        let (event_type, id) = if let Some((ty, id)) = message.key.split_once('-') {
            (ty.to_string(), id.parse().context("Failed to parse event ID from Kafka message key")?)
        } else {
            anyhow::bail!("Kafka message key is not in expected format 'EventType-EventID'");
        };
        Ok(Self {
            id,
            event_type,
            payload,
            received_at: Utc::now(),
        })
    }
}

/// Enriched transaction with correlated lifecycle events
#[derive(sqlx::FromRow, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnrichedTransaction {
    pub tx_hash: String,
    pub accepted_at: DateTime<Utc>,
    pub processed_at: DateTime<Utc>,
    pub sender: String,
    pub status: String,
    pub raw_accepted: serde_json::Value,
    pub raw_processed: serde_json::Value,
}

/// App-specific event with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppEvent {
    pub event_id: String,
    pub tx_hash: String,
    pub app_type: String,
    pub topic: String,
    pub timestamp: DateTime<Utc>,
    pub lifecycle: TransactionLifecycle,
    pub app_data: serde_json::Value,
}

/// Transaction lifecycle data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionLifecycle {
    pub accepted_at: DateTime<Utc>,
    pub processed_at: DateTime<Utc>,
    pub status: String,
    pub sender: String,
}
