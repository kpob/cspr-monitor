use anyhow::Result;
use casper_common::{AppEvent, EnrichedTransaction};

pub mod contracts;
pub mod exchanges;

/// Trait for identifying application patterns in transactions
pub trait AppIdentifier: Send + Sync {
    /// Name of this identifier
    fn name(&self) -> &'static str;

    /// Kafka topic to publish identified events to
    fn topic(&self) -> &'static str;

    /// Identify if this transaction matches the app pattern
    ///
    /// Returns Some(AppEvent) if the transaction matches this app pattern
    /// Returns None if it doesn't match
    fn identify(&self, tx: &EnrichedTransaction) -> Result<Option<AppEvent>>;

    /// Reload configuration from the source (e.g. file on disk).
    /// Called periodically so identifiers can pick up files written after startup.
    /// Default implementation is a no-op.
    fn reload(&self) {}
}

/// Registry of app identifiers
pub struct IdentifierRegistry {
    identifiers: Vec<Box<dyn AppIdentifier>>,
}

impl IdentifierRegistry {
    /// Create a new identifier registry with default identifiers
    pub fn new() -> Self {
        Self {
            identifiers: vec![
                Box::new(contracts::ContractPatternIdentifier::new()),
                Box::new(exchanges::ExchangeWalletIdentifier::new()),
            ],
        }
    }

    /// Reload all identifiers from their configuration sources
    pub fn reload_all(&self) {
        for identifier in &self.identifiers {
            identifier.reload();
        }
    }

    /// Run all identifiers on a transaction and collect matching events
    pub fn identify_all(&self, tx: &EnrichedTransaction) -> Result<Vec<AppEvent>> {
        let mut events = Vec::new();

        for identifier in &self.identifiers {
            if let Some(app_event) = identifier.identify(tx)? {
                tracing::debug!(
                    "Identifier '{}' matched tx_hash: {}",
                    identifier.name(),
                    tx.tx_hash
                );
                events.push(app_event);
            }
        }

        Ok(events)
    }
}

impl Default for IdentifierRegistry {
    fn default() -> Self {
        Self::new()
    }
}
