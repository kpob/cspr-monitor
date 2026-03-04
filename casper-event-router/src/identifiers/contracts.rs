use anyhow::Result;
use casper_common::{APPS_CONTRACTS, AppEvent, EnrichedTransaction, TransactionLifecycle};
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::RwLock;

use super::AppIdentifier;

type ContractHash = String;
type ContractName = String;

#[derive(Debug, Deserialize)]
struct ContractConfig {
    contracts: HashMap<ContractName, ContractHash>,
}

/// Identifies transactions targeting specific smart contracts
#[derive(Debug, Default)]
pub struct ContractPatternIdentifier {
    // Maps contract hash -> contract name
    contracts: RwLock<HashMap<String, String>>,
    config_path: Option<String>,
}

impl ContractPatternIdentifier {
    pub fn new() -> Self {
        let config_path = std::env::var("DEPLOYED_CONTRACTS_JSON_PATH").ok();
        let contracts = config_path
            .as_deref()
            .and_then(|path| match Self::load_from_file(path) {
                Ok(deployed) => {
                    tracing::info!("Loaded {} deployed contracts from {}", deployed.len(), path);
                    Some(deployed)
                }
                Err(e) => {
                    tracing::warn!("Could not load deployed contracts from {}: {}", path, e);
                    None
                }
            })
            .unwrap_or_default();

        tracing::info!(
            "Contract pattern identifier initialized with {} contracts",
            contracts.len()
        );

        Self {
            contracts: RwLock::new(contracts),
            config_path,
        }
    }

    fn load_from_file(path: &str) -> Result<HashMap<String, String>> {
        let path = Path::new(path);
        if !path.exists() {
            return Err(anyhow::anyhow!("Config file not found: {}", path.display()));
        }

        let content = fs::read_to_string(path)?;
        let config: ContractConfig = serde_json::from_str(&content)?;

        // Reverse the mapping: name->hash becomes hash->name for efficient lookups.
        // Normalize hash: strip address prefixes (e.g. "hash-<hex>") so the key is plain hex,
        // matching what the correlator extracts from transaction payloads.
        Ok(config.contracts.into_iter().map(|(name, hash)| {
            let normalized = if hash.contains('-') {
                hash.rsplit_once('-').map(|(_, h)| h.to_string()).unwrap_or(hash)
            } else {
                hash
            };
            (normalized, name)
        }).collect())
    }
}

impl AppIdentifier for ContractPatternIdentifier {
    fn name(&self) -> &'static str {
        "contract_pattern"
    }

    fn topic(&self) -> &'static str {
        APPS_CONTRACTS
    }

    fn reload(&self) {
        if let Some(path) = &self.config_path {
            match Self::load_from_file(path) {
                Ok(loaded) => {
                    let count = loaded.len();
                    *self.contracts.write().unwrap() = loaded;
                    tracing::info!("Reloaded {} contracts from {}", count, path);
                }
                Err(e) => tracing::debug!("Could not reload contracts from {}: {}", path, e),
            }
        }
    }

    fn identify(&self, tx: &EnrichedTransaction) -> Result<Option<AppEvent>> {
        // Use the contract hash already extracted by the correlator using casper-types.
        let contract_hash = match &tx.contract_hash {
            Some(h) => h.clone(),
            None => return Ok(None),
        };

        // Check if this contract is in our watch list
        let contract_name = match self.contracts.read().unwrap().get(&contract_hash).cloned() {
            Some(name) => name,
            None => return Ok(None),
        };

        // Build app-specific data
        let mut app_data = serde_json::Map::new();
        app_data.insert("contract_hash".to_string(), serde_json::json!(contract_hash));
        app_data.insert("contract_name".to_string(), serde_json::json!(contract_name));

        if let Some(ep) = &tx.entry_point {
            app_data.insert("entry_point".to_string(), serde_json::json!(ep));
        }

        if let Some(args) = &tx.args {
            app_data.insert("args".to_string(), args.clone());
        }

        // Create the app event
        let app_event = AppEvent {
            event_id: format!("contract-{}-{}", tx.tx_hash, Utc::now().timestamp_millis()),
            tx_hash: tx.tx_hash.clone(),
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

    fn make_tx(
        tx_hash: &str,
        contract_hash: Option<&str>,
        entry_point: Option<&str>,
    ) -> EnrichedTransaction {
        EnrichedTransaction {
            tx_hash: tx_hash.to_string(),
            sender: "sender".to_string(),
            accepted_at: Utc::now(),
            processed_at: Utc::now(),
            status: "success".to_string(),
            raw_accepted: serde_json::json!({}),
            raw_processed: serde_json::json!({}),
            contract_hash: contract_hash.map(str::to_string),
            entry_point: entry_point.map(str::to_string),
            args: None,
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
    fn identify_matches_known_contract_hash() {
        let identifier = ContractPatternIdentifier {
            contracts: RwLock::new(
                [("abc123".to_string(), "MyContract".to_string())]
                    .into_iter()
                    .collect(),
            ),
            config_path: None,
        };

        let event = identifier
            .identify(&make_tx("tx1", Some("abc123"), Some("transfer")))
            .unwrap()
            .unwrap();

        assert_eq!(event.app_data["contract_hash"], "abc123");
        assert_eq!(event.app_data["contract_name"], "MyContract");
        assert_eq!(event.app_data["entry_point"], "transfer");
    }

    #[test]
    fn identify_returns_none_for_unknown_contract() {
        let identifier = ContractPatternIdentifier {
            contracts: RwLock::new(HashMap::new()),
            config_path: None,
        };

        assert!(identifier
            .identify(&make_tx("tx1", Some("unknown"), None))
            .unwrap()
            .is_none());
    }

    #[test]
    fn identify_returns_none_when_no_contract_hash() {
        let identifier = ContractPatternIdentifier {
            contracts: RwLock::new(
                [("abc123".to_string(), "MyContract".to_string())]
                    .into_iter()
                    .collect(),
            ),
            config_path: None,
        };

        assert!(identifier
            .identify(&make_tx("tx1", None, None))
            .unwrap()
            .is_none());
    }

    #[test]
    fn identify_omits_entry_point_when_absent() {
        let identifier = ContractPatternIdentifier {
            contracts: RwLock::new(
                [("abc123".to_string(), "MyContract".to_string())]
                    .into_iter()
                    .collect(),
            ),
            config_path: None,
        };

        let event = identifier
            .identify(&make_tx("tx1", Some("abc123"), None))
            .unwrap()
            .unwrap();

        assert!(event.app_data.get("entry_point").is_none());
    }

    #[test]
    fn identify_includes_args_in_app_data() {
        let identifier = ContractPatternIdentifier {
            contracts: RwLock::new(
                [("abc123".to_string(), "MyContract".to_string())]
                    .into_iter()
                    .collect(),
            ),
            config_path: None,
        };

        let mut tx = make_tx("tx1", Some("abc123"), Some("mint"));
        tx.args = Some(serde_json::json!({"address": "account-hash-abc", "amount": "1000"}));

        let event = identifier.identify(&tx).unwrap().unwrap();

        assert_eq!(event.app_data["args"]["address"], "account-hash-abc");
        assert_eq!(event.app_data["args"]["amount"], "1000");
    }

    #[test]
    fn reload_picks_up_updated_file() {
        let path = write_temp(
            "test_contracts_reload.json",
            r#"{"contracts":{"OldContract":"aaa111"}}"#,
        );

        let identifier = ContractPatternIdentifier {
            contracts: RwLock::new(HashMap::new()),
            config_path: Some(path.clone()),
        };

        identifier.reload();
        assert!(identifier.contracts.read().unwrap().contains_key("aaa111"));

        // Overwrite with new data
        std::fs::write(&path, r#"{"contracts":{"NewContract":"bbb222"}}"#).unwrap();
        identifier.reload();

        let contracts = identifier.contracts.read().unwrap();
        assert!(!contracts.contains_key("aaa111"));
        assert!(contracts.contains_key("bbb222"));
    }
}
