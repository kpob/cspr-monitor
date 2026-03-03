use anyhow::Result;
use casper_common::{APPS_EXCHANGES, AppEvent, EnrichedTransaction, TransactionLifecycle};
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::RwLock;

use super::AppIdentifier;

type AccountName = String;
type AccountAddress = String;

#[derive(Debug, Deserialize)]
struct ExchangeConfig {
    exchanges: HashMap<AccountAddress, AccountName>,
}

/// Identifies transactions from/to known exchange wallets
pub struct ExchangeWalletIdentifier {
    exchange_addresses: RwLock<HashMap<AccountAddress, AccountName>>,
    config_path: Option<String>,
}

impl ExchangeWalletIdentifier {
    pub fn new() -> Self {
        let config_path = std::env::var("EXCHANGE_CONFIG_JSON_PATH").ok();
        let exchange_addresses = config_path
            .as_deref()
            .and_then(|path| match Self::load_from_file(path) {
                Ok(exchanges) => {
                    tracing::info!("Loaded {} exchanges from {}", exchanges.len(), path);
                    Some(exchanges)
                }
                Err(e) => {
                    tracing::warn!("Could not load exchanges from {}: {}", path, e);
                    None
                }
            })
            .unwrap_or_default();

        tracing::info!(
            "Exchange wallet identifier initialized with {} exchanges",
            exchange_addresses.len()
        );

        Self {
            exchange_addresses: RwLock::new(exchange_addresses),
            config_path,
        }
    }

    fn load_from_file(path: &str) -> Result<HashMap<String, String>> {
        let path = Path::new(path);
        if !path.exists() {
            return Err(anyhow::anyhow!("Config file not found: {}", path.display()));
        }

        let content = fs::read_to_string(path)?;
        let config: ExchangeConfig = serde_json::from_str(&content)?;

        // Normalize address: strip account prefixes (e.g. "account-hash-<hex>") so the key
        // is plain hex, matching what appears in EnrichedTransaction::sender.
        Ok(config.exchanges.into_iter().map(|(addr, name)| {
            let normalized = if addr.contains('-') {
                addr.rsplit_once('-').map(|(_, h)| h.to_string()).unwrap_or(addr)
            } else {
                addr
            };
            (normalized, name)
        }).collect())
    }
}

impl Default for ExchangeWalletIdentifier {
    fn default() -> Self {
        Self::new()
    }
}

impl AppIdentifier for ExchangeWalletIdentifier {
    fn name(&self) -> &'static str {
        "exchange_wallet"
    }

    fn topic(&self) -> &'static str {
        APPS_EXCHANGES
    }

    fn reload(&self) {
        if let Some(path) = &self.config_path {
            match Self::load_from_file(path) {
                Ok(loaded) => {
                    let count = loaded.len();
                    *self.exchange_addresses.write().unwrap() = loaded;
                    tracing::info!("Reloaded {} exchanges from {}", count, path);
                }
                Err(e) => tracing::debug!("Could not reload exchanges from {}: {}", path, e),
            }
        }
    }

    fn identify(&self, tx: &EnrichedTransaction) -> Result<Option<AppEvent>> {
        // Check if sender is a known exchange
        let exchange_name = match self.exchange_addresses.read().unwrap().get(&tx.sender).cloned() {
            Some(name) => name,
            None => return Ok(None),
        };

        // Build app-specific data
        let mut app_data = serde_json::Map::new();
        app_data.insert("exchange".to_string(), serde_json::json!(exchange_name));
        app_data.insert("wallet_address".to_string(), serde_json::json!(tx.sender));

        // Try to extract amount/value if available
        if let Some(amount) = tx.raw_accepted
            .get("TransactionAccepted")
            .and_then(|ta| ta.get("Version1"))
            .and_then(|v1| v1.get("payload"))
            .and_then(|p| p.get("fields"))
            .and_then(|f| f.get("amount"))
        {
            app_data.insert("amount".to_string(), amount.clone());
        }

        // Create the app event
        let app_event = AppEvent {
            event_id: format!("exchange-{}-{}", tx.tx_hash, Utc::now().timestamp_millis()),
            tx_hash: tx.tx_hash.clone(),
            app_type: "exchange_activity".to_string(),
            topic: self.topic().to_string(),
            timestamp: Utc::now(),
            lifecycle: TransactionLifecycle {
                accepted_at: tx.accepted_at,
                processed_at: tx.processed_at,
                status: tx.status.clone(),
                sender: tx.sender.clone(),
            },
            app_data: serde_json::Value::Object(app_data),
        };

        Ok(Some(app_event))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use casper_common::EnrichedTransaction;
    use chrono::Utc;

    fn write_temp(filename: &str, content: &str) -> String {
        let path = std::env::temp_dir().join(filename);
        std::fs::write(&path, content).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn make_tx(tx_hash: &str, sender: &str) -> EnrichedTransaction {
        EnrichedTransaction {
            tx_hash: tx_hash.to_string(),
            sender: sender.to_string(),
            accepted_at: Utc::now(),
            processed_at: Utc::now(),
            status: "success".to_string(),
            raw_accepted: serde_json::json!({}),
            raw_processed: serde_json::json!({}),
            contract_hash: None,
            entry_point: None,
            args: None,
        }
    }

    #[test]
    fn load_from_file_parses_valid_json() {
        let path = write_temp(
            "test_exchanges_valid.json",
            r#"{"exchanges":{"addr123":"Binance"}}"#,
        );
        let result = ExchangeWalletIdentifier::load_from_file(&path).unwrap();
        assert_eq!(result.get("addr123").map(String::as_str), Some("Binance"));
    }

    #[test]
    fn load_from_file_strips_account_hash_prefix() {
        let path = write_temp(
            "test_exchanges_prefix.json",
            r#"{"exchanges":{"account-hash-abc123":"Binance"}}"#,
        );
        let result = ExchangeWalletIdentifier::load_from_file(&path).unwrap();
        assert_eq!(result.get("abc123").map(String::as_str), Some("Binance"));
    }

    #[test]
    fn load_from_file_returns_err_for_missing_file() {
        let result = ExchangeWalletIdentifier::load_from_file("/nonexistent/exchanges.json");
        assert!(result.is_err());
    }

    #[test]
    fn load_from_file_returns_err_for_invalid_json() {
        let path = write_temp("test_exchanges_invalid.json", "not json");
        let result = ExchangeWalletIdentifier::load_from_file(&path);
        assert!(result.is_err());
    }

    #[test]
    fn identify_matches_known_exchange_sender() {
        let identifier = ExchangeWalletIdentifier {
            exchange_addresses: RwLock::new(
                [("addr123".to_string(), "Binance".to_string())]
                    .into_iter()
                    .collect(),
            ),
            config_path: None,
        };

        let event = identifier
            .identify(&make_tx("tx1", "addr123"))
            .unwrap()
            .unwrap();

        assert_eq!(event.app_type, "exchange_activity");
        assert_eq!(event.app_data["exchange"], "Binance");
        assert_eq!(event.app_data["wallet_address"], "addr123");
    }

    #[test]
    fn identify_returns_none_for_unknown_sender() {
        let identifier = ExchangeWalletIdentifier {
            exchange_addresses: RwLock::new(HashMap::new()),
            config_path: None,
        };

        assert!(identifier
            .identify(&make_tx("tx1", "unknown"))
            .unwrap()
            .is_none());
    }

    #[test]
    fn reload_picks_up_updated_file() {
        let path = write_temp(
            "test_exchanges_reload.json",
            r#"{"exchanges":{"old_addr":"OldExchange"}}"#,
        );

        let identifier = ExchangeWalletIdentifier {
            exchange_addresses: RwLock::new(HashMap::new()),
            config_path: Some(path.clone()),
        };

        identifier.reload();
        assert!(identifier.exchange_addresses.read().unwrap().contains_key("old_addr"));

        // Overwrite with new data
        std::fs::write(&path, r#"{"exchanges":{"new_addr":"NewExchange"}}"#).unwrap();
        identifier.reload();

        let addresses = identifier.exchange_addresses.read().unwrap();
        assert!(!addresses.contains_key("old_addr"));
        assert!(addresses.contains_key("new_addr"));
    }
}
