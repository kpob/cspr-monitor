use anyhow::Result;
use casper_common::{APPS_CONTRACTS, AppEvent, EnrichedTransaction, TransactionLifecycle};
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::AppIdentifier;

type ContractHash = String;
type ContractName = String;

#[derive(Debug, Deserialize)]
struct ContractConfig {
    contracts: HashMap<ContractName, ContractHash>,
}

/// Identifies transactions targeting specific smart contracts
pub struct ContractPatternIdentifier {
    // Maps contract hash -> contract name
    contracts: HashMap<String, String>,
}

impl ContractPatternIdentifier {
    pub fn new() -> Self {
        let contracts = Self::load_contracts();

        tracing::info!(
            "Contract pattern identifier initialized with {} contracts",
            contracts.len()
        );

        Self { contracts }
    }

    fn load_contracts() -> HashMap<String, String> {
        let config_path = std::env::var("CONTRACT_CONFIG_PATH")
            .unwrap_or_else(|_| "casper-event-router/config/known_contracts.json".to_string());

        match Self::load_from_file(&config_path) {
            Ok(contracts) => {
                tracing::info!("Loaded {} contracts from file: {}", contracts.len(), config_path);
                contracts
            },
            Err(e) => {
                tracing::warn!("Failed to load contracts from file: {}. Error: {}", config_path, e);
                HashMap::new() // Return empty if no config found or failed to load
            }
        }
    }

    fn load_from_file(path: &str) -> Result<HashMap<String, String>> {
        let path = Path::new(path);
        if !path.exists() {
            return Err(anyhow::anyhow!("Config file not found: {}", path.display()));
        }

        let content = fs::read_to_string(path)?;
        let config: ContractConfig = serde_json::from_str(&content)?;

        // Reverse the mapping: name->hash becomes hash->name for efficient lookups
        Ok(config.contracts.into_iter().map(|(name, hash)| (hash, name)).collect())
    }

    fn extract_contract_hash(&self, tx: &EnrichedTransaction) -> Option<String> {
        // Try V1 transaction format
        let v1 = tx.raw_accepted
            .get("TransactionAccepted")
            .and_then(|ta| ta.get("Version1"))
            .and_then(|v1| v1.get("payload"))
            .and_then(|p| p.get("fields"))
            .and_then(|f| f.get("target"))
            .and_then(|t| t.get("Stored"))
            .and_then(|s| s.get("id"))
            .and_then(|id| id.get("ByPackageHash"))
            .and_then(|bph| bph.get("addr"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if v1.is_some() {
            return v1;
        }

        // Try Deploy format
        tx.raw_accepted
            .get("TransactionAccepted")
            .and_then(|ta| ta.get("Deploy"))
            .and_then(|d| d.get("session"))
            .and_then(|s| s.get("StoredContractByHash"))
            .and_then(|sc| sc.get("hash"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }
}

impl AppIdentifier for ContractPatternIdentifier {
    fn name(&self) -> &'static str {
        "contract_pattern"
    }

    fn topic(&self) -> &'static str {
        APPS_CONTRACTS
    }

    fn identify(&self, tx: &EnrichedTransaction) -> Result<Option<AppEvent>> {
        // Extract contract hash from transaction
        let contract_hash = match self.extract_contract_hash(tx) {
            Some(hash) => hash,
            None => return Ok(None),
        };

        // Check if this contract is in our watch list
        let contract_name = match self.contracts.get(&contract_hash) {
            Some(name) => name.clone(),
            None => return Ok(None),
        };

        // Build app-specific data
        let mut app_data = serde_json::Map::new();
        app_data.insert("contract_hash".to_string(), serde_json::json!(contract_hash));
        app_data.insert("contract_name".to_string(), serde_json::json!(contract_name));

        // Try to extract method/entry point
        if let Some(entry_point) = tx.raw_accepted
            .get("TransactionAccepted")
            .and_then(|ta| ta.get("Version1"))
            .and_then(|v1| v1.get("payload"))
            .and_then(|p| p.get("fields"))
            .and_then(|f| f.get("entry_point"))
            .and_then(|ep| ep.as_str())
        {
            app_data.insert("entry_point".to_string(), serde_json::json!(entry_point));
        }

        // Create the app event
        let app_event = AppEvent {
            event_id: format!("contract-{}-{}", tx.tx_hash, Utc::now().timestamp_millis()),
            tx_hash: tx.tx_hash.clone(),
            app_type: "contract_interaction".to_string(),
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

impl Default for ContractPatternIdentifier {
    fn default() -> Self {
        Self::new()
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

    fn make_tx(tx_hash: &str, raw_accepted: serde_json::Value) -> EnrichedTransaction {
        EnrichedTransaction {
            tx_hash: tx_hash.to_string(),
            sender: "sender".to_string(),
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
            "test_contracts_valid.json",
            r#"{"contracts":{"MyContract":"abc123"}}"#,
        );
        let result = ContractPatternIdentifier::load_from_file(&path).unwrap();
        // mapping is inverted: name->hash becomes hash->name
        assert_eq!(result.get("abc123").map(String::as_str), Some("MyContract"));
    }

    #[test]
    fn load_from_file_returns_err_for_missing_file() {
        let result = ContractPatternIdentifier::load_from_file("/nonexistent/contracts.json");
        assert!(result.is_err());
    }

    #[test]
    fn load_from_file_returns_err_for_invalid_json() {
        let path = write_temp("test_contracts_invalid.json", "not json");
        let result = ContractPatternIdentifier::load_from_file(&path);
        assert!(result.is_err());
    }

    #[test]
    fn identify_matches_v1_transaction() {
        let identifier = ContractPatternIdentifier {
            contracts: [("abc123".to_string(), "MyContract".to_string())]
                .into_iter()
                .collect(),
        };

        let raw_accepted = serde_json::json!({
            "TransactionAccepted": {
                "Version1": {
                    "payload": {
                        "fields": {
                            "target": {
                                "Stored": {
                                    "id": { "ByPackageHash": { "addr": "abc123" } }
                                }
                            },
                            "entry_point": "transfer"
                        }
                    }
                }
            }
        });

        let event = identifier.identify(&make_tx("tx1", raw_accepted)).unwrap().unwrap();
        assert_eq!(event.app_type, "contract_interaction");
        assert_eq!(event.app_data["contract_hash"], "abc123");
        assert_eq!(event.app_data["contract_name"], "MyContract");
        assert_eq!(event.app_data["entry_point"], "transfer");
    }

    #[test]
    fn identify_matches_deploy_transaction() {
        let identifier = ContractPatternIdentifier {
            contracts: [("abc123".to_string(), "MyContract".to_string())]
                .into_iter()
                .collect(),
        };

        let raw_accepted = serde_json::json!({
            "TransactionAccepted": {
                "Deploy": {
                    "session": {
                        "StoredContractByHash": { "hash": "abc123" }
                    }
                }
            }
        });

        let event = identifier.identify(&make_tx("tx1", raw_accepted)).unwrap().unwrap();
        assert_eq!(event.app_data["contract_hash"], "abc123");
        assert_eq!(event.app_data["contract_name"], "MyContract");
    }

    #[test]
    fn identify_returns_none_for_unknown_contract() {
        let identifier = ContractPatternIdentifier {
            contracts: HashMap::new(),
        };

        let raw_accepted = serde_json::json!({
            "TransactionAccepted": {
                "Version1": {
                    "payload": {
                        "fields": {
                            "target": {
                                "Stored": {
                                    "id": { "ByPackageHash": { "addr": "unknown" } }
                                }
                            }
                        }
                    }
                }
            }
        });

        assert!(identifier.identify(&make_tx("tx1", raw_accepted)).unwrap().is_none());
    }

    #[test]
    fn identify_returns_none_when_no_contract_hash() {
        let identifier = ContractPatternIdentifier {
            contracts: [("abc123".to_string(), "MyContract".to_string())]
                .into_iter()
                .collect(),
        };

        let raw_accepted = serde_json::json!({ "TransactionAccepted": {} });
        assert!(identifier.identify(&make_tx("tx1", raw_accepted)).unwrap().is_none());
    }
}
