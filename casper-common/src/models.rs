use serde::{Deserialize, Serialize};
use sqlx::types::chrono::{DateTime, Utc};

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

/// Transaction with both accepted and processed data
#[derive(sqlx::FromRow, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedTx {
    pub tx_hash: String,
    pub accepted_at: DateTime<Utc>,
    pub processed_at: DateTime<Utc>,
    pub status: String,
    pub sender: String,
    pub raw_accepted: serde_json::Value,
    pub raw_processed: serde_json::Value,
}

/// Enriched transaction with correlated lifecycle events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedTransaction {
    pub tx_hash: String,
    pub accepted_at: DateTime<Utc>,
    pub processed_at: DateTime<Utc>,
    pub sender: String,
    pub status: String,
    pub raw_accepted: serde_json::Value,
    pub raw_processed: serde_json::Value,
}

impl From<AcceptedTx> for EnrichedTransaction {
    fn from(tx: AcceptedTx) -> Self {
        Self {
            tx_hash: tx.tx_hash,
            accepted_at: tx.accepted_at,
            processed_at: tx.processed_at,
            sender: tx.sender,
            status: tx.status,
            raw_accepted: tx.raw_accepted,
            raw_processed: tx.raw_processed,
        }
    }
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
