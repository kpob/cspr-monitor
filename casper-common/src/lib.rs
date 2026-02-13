pub mod database;
pub mod kafka;
pub mod models;
pub mod topics;
mod tracing;

// Re-export commonly used types
pub use database::{Database, PostgresDB};
pub use kafka::{KafkaConsumer, KafkaMessage, KafkaProducer};
pub use models::{
    AcceptedTx, AppEvent, EnrichedTransaction, RawEvent, TransactionLifecycle,
    BLOCK_ADDED, TRANSACTION_ACCEPTED, TRANSACTION_PROCESSED,
};
pub use topics::{
    APPS_CONTRACTS, APPS_EXCHANGES, ENRICHED_CHAIN_EVENTS, RAW_CHAIN_EVENTS, SIGNALS_ARBITRAGE,
};
pub use tracing::init_tracing;
