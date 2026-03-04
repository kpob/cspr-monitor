use anyhow::Result;
use casper_common::{APPS_EXCHANGES, APPS_UNCLASSIFIED, AppEvent, EnrichedTransaction, TransactionLifecycle};
use chrono::Utc;
use std::sync::{Arc, RwLock};

pub mod contracts;
pub mod exchanges;

use contracts::{PerAppIdentifier, load_app_identifiers};

const DEFAULT_CONFIG_PATH: &str = "config/apps_config.json";

/// Registry of app identifiers.
///
/// Per-app contract identifiers are loaded dynamically from the unified config and
/// rebuilt on every `reload_all()` call. The exchange identifier is reloaded in-place.
pub struct IdentifierRegistry {
    app_identifiers: Arc<RwLock<Vec<PerAppIdentifier>>>,
    exchange_identifier: exchanges::ExchangeWalletIdentifier,
    config_path: String,
}

impl IdentifierRegistry {
    pub fn new() -> Self {
        let config_path = std::env::var("APPS_CONFIG_PATH")
            .unwrap_or_else(|_| DEFAULT_CONFIG_PATH.to_string());

        let app_identifiers = match load_app_identifiers(&config_path) {
            Ok(ids) => {
                tracing::info!(
                    "Loaded {} app identifiers from {}",
                    ids.len(),
                    config_path
                );
                ids
            }
            Err(e) => {
                tracing::warn!(
                    "Could not load app identifiers from {}: {}",
                    config_path,
                    e
                );
                vec![]
            }
        };

        let exchange_identifier = exchanges::ExchangeWalletIdentifier::new(&config_path);

        Self {
            app_identifiers: Arc::new(RwLock::new(app_identifiers)),
            exchange_identifier,
            config_path,
        }
    }

    /// Reload all identifiers from the unified config file.
    pub fn reload_all(&self) {
        match load_app_identifiers(&self.config_path) {
            Ok(ids) => {
                let count = ids.len();
                *self.app_identifiers.write().unwrap() = ids;
                tracing::info!(
                    "Reloaded {} app identifiers from {}",
                    count,
                    self.config_path
                );
            }
            Err(e) => tracing::debug!(
                "Could not reload app identifiers from {}: {}",
                self.config_path,
                e
            ),
        }

        self.exchange_identifier.reload();
    }

    /// Run all identifiers on a transaction and collect `(topic, event)` pairs.
    ///
    /// Contract transactions with no matching app are emitted to `apps.unclassified`.
    pub fn identify_all(&self, tx: &EnrichedTransaction) -> Result<Vec<(String, AppEvent)>> {
        let mut events = Vec::new();

        // Per-app contract identifiers
        let app_ids = self.app_identifiers.read().unwrap();
        let mut contract_matched = false;
        for identifier in app_ids.iter() {
            if let Some(event) = identifier.identify(tx)? {
                tracing::debug!(
                    "App '{}' matched tx_hash: {}",
                    identifier.name,
                    tx.tx_hash
                );
                events.push((identifier.topic.clone(), event));
                contract_matched = true;
            }
        }
        drop(app_ids);

        // Unclassified fallback: contract tx that didn't match any known app
        if !contract_matched {
            if let Some(contract_hash) = &tx.contract_hash {
                events.push((
                    APPS_UNCLASSIFIED.to_string(),
                    build_unclassified_event(tx, contract_hash),
                ));
            }
        }

        // Exchange identifier
        if let Some(event) = self.exchange_identifier.identify(tx)? {
            tracing::debug!("Exchange identifier matched tx_hash: {}", tx.tx_hash);
            events.push((APPS_EXCHANGES.to_string(), event));
        }

        Ok(events)
    }
}

impl Default for IdentifierRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn build_unclassified_event(tx: &EnrichedTransaction, contract_hash: &str) -> AppEvent {
    let mut app_data = serde_json::Map::new();
    app_data.insert(
        "contract_hash".to_string(),
        serde_json::json!(contract_hash),
    );
    if let Some(ep) = &tx.entry_point {
        app_data.insert("entry_point".to_string(), serde_json::json!(ep));
    }
    if let Some(args) = &tx.args {
        app_data.insert("args".to_string(), args.clone());
    }
    AppEvent {
        event_id: format!(
            "unclassified-{}-{}",
            tx.tx_hash,
            Utc::now().timestamp_millis()
        ),
        tx_hash: tx.tx_hash.clone(),
        timestamp: Utc::now(),
        lifecycle: TransactionLifecycle {
            accepted_at: tx.accepted_at,
            processed_at: tx.processed_at,
            status: tx.status.clone(),
            sender: tx.sender.clone(),
        },
        app_data: serde_json::Value::Object(app_data),
    }
}
