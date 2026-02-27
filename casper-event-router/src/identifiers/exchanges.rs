use anyhow::Result;
use casper_common::{APPS_CONTRACTS, AppEvent, EnrichedTransaction, TransactionLifecycle};
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::AppIdentifier;

#[derive(Debug, Deserialize)]
struct ExchangeConfig {
    exchanges: HashMap<String, String>,
}

/// Identifies transactions from/to known exchange wallets
#[derive(Debug, Default)]
pub struct ExchangeWalletIdentifier {
    exchange_addresses: HashMap<String, String>,
}

impl ExchangeWalletIdentifier {
    pub fn new() -> Self {
        let exchange_addresses = Self::load_exchanges();
        tracing::info!(
            "Exchange wallet identifier initialized with {} exchanges",
            exchange_addresses.len()
        );

        Self { exchange_addresses }
    }

    fn load_exchanges() -> HashMap<String, String> {
        let config_path = std::env::var("EXCHANGE_CONFIG_PATH")
            .unwrap_or_else(|_| "casper-event-router/config/exchanges.json".to_string());

        if let Ok(exchanges) = Self::load_from_file(&config_path) {
            tracing::info!("Loaded {} exchanges from file: {}", exchanges.len(), config_path);
            return exchanges;
        }

        HashMap::new() // Return empty if no config found
    }

    fn load_from_file(path: &str) -> Result<HashMap<String, String>> {
        let path = Path::new(path);
        if !path.exists() {
            return Err(anyhow::anyhow!("Config file not found: {}", path.display()));
        }

        let content = fs::read_to_string(path)?;
        let config: ExchangeConfig = serde_json::from_str(&content)?;
        Ok(config.exchanges)
    }

    fn identify_exchange(&self, sender: &str) -> Option<&String> {
        self.exchange_addresses.get(sender)
    }
}

impl AppIdentifier for ExchangeWalletIdentifier {
    fn name(&self) -> &'static str {
        "exchange_wallet"
    }

    fn topic(&self) -> &'static str {
        APPS_CONTRACTS
    }

    fn identify(&self, tx: &EnrichedTransaction) -> Result<Option<AppEvent>> {
        // Check if sender is a known exchange
        let exchange_name = match self.identify_exchange(&tx.sender) {
            Some(name) => name.clone(),
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

    fn make_tx(tx_hash: &str, sender: &str, raw_accepted: serde_json::Value) -> EnrichedTransaction {
        EnrichedTransaction {
            tx_hash: tx_hash.to_string(),
            sender: sender.to_string(),
            accepted_at: Utc::now(),
            processed_at: Utc::now(),
            status: "success".to_string(),
            raw_accepted,
            raw_processed: serde_json::json!({}),
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
            exchange_addresses: [("addr123".to_string(), "Binance".to_string())]
                .into_iter()
                .collect(),
        };

        let event = identifier
            .identify(&make_tx("tx1", "addr123", serde_json::json!({})))
            .unwrap()
            .unwrap();

        assert_eq!(event.app_type, "exchange_activity");
        assert_eq!(event.app_data["exchange"], "Binance");
        assert_eq!(event.app_data["wallet_address"], "addr123");
    }

    #[test]
    fn identify_returns_none_for_unknown_sender() {
        let identifier = ExchangeWalletIdentifier {
            exchange_addresses: HashMap::new(),
        };

        assert!(identifier
            .identify(&make_tx("tx1", "unknown", serde_json::json!({})))
            .unwrap()
            .is_none());
    }

    #[test]
    fn identify_includes_amount_when_present() {
        let identifier = ExchangeWalletIdentifier {
            exchange_addresses: [("addr123".to_string(), "Binance".to_string())]
                .into_iter()
                .collect(),
        };

        let raw_accepted = serde_json::json!({
            "TransactionAccepted": {
                "Version1": {
                    "payload": {
                        "fields": { "amount": "500000000" }
                    }
                }
            }
        });

        let event = identifier
            .identify(&make_tx("tx1", "addr123", raw_accepted))
            .unwrap()
            .unwrap();

        assert_eq!(event.app_data["amount"], "500000000");
    }
}
