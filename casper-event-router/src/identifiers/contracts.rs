use anyhow::Result;
use casper_common::{AppEvent, EnrichedTransaction, TransactionLifecycle};
use chrono::Utc;
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub name: String,
    pub topic: String,
    pub contract_hash: String,
}

#[derive(Debug, Deserialize)]
struct AppsFile {
    apps: Vec<AppConfig>,
}

/// Identifier for a single app matched by its contract hash.
#[derive(Debug, Clone)]
pub struct PerAppIdentifier {
    pub name: String,
    pub topic: String,
    pub contract_hash: String, // normalized lowercase hex
}

impl PerAppIdentifier {
    fn from_config(app: AppConfig) -> Self {
        Self {
            name: app.name,
            topic: app.topic,
            contract_hash: normalize_hash(&app.contract_hash),
        }
    }

    pub fn identify(&self, tx: &EnrichedTransaction) -> Result<Option<AppEvent>> {
        let contract_hash = match &tx.contract_hash {
            Some(h) => h.clone(),
            None => return Ok(None),
        };

        if contract_hash != self.contract_hash {
            return Ok(None);
        }

        let mut app_data = serde_json::Map::new();
        app_data.insert("contract_hash".to_string(), serde_json::json!(contract_hash));
        app_data.insert("contract_name".to_string(), serde_json::json!(&self.name));

        if let Some(ep) = &tx.entry_point {
            app_data.insert("entry_point".to_string(), serde_json::json!(ep));
        }

        if let Some(args) = &tx.args {
            app_data.insert("args".to_string(), args.clone());
        }

        Ok(Some(AppEvent {
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
        }))
    }
}

/// Load per-app identifiers from the `apps` array in the unified config file.
pub fn load_app_identifiers(path: &str) -> Result<Vec<PerAppIdentifier>> {
    let p = Path::new(path);
    if !p.exists() {
        return Err(anyhow::anyhow!("Config file not found: {}", p.display()));
    }

    let content = fs::read_to_string(p)?;
    let config: AppsFile = serde_json::from_str(&content)?;

    Ok(config.apps.into_iter().map(PerAppIdentifier::from_config).collect())
}

fn normalize_hash(hash: &str) -> String {
    if hash.contains('-') {
        hash.rsplit_once('-')
            .map(|(_, h)| h.to_string())
            .unwrap_or_else(|| hash.to_string())
    } else {
        hash.to_string()
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
    fn load_app_identifiers_parses_valid_json() {
        let path = write_temp(
            "test_apps_valid.json",
            r#"{"apps":[{"name":"MyApp","topic":"apps.myapp","contract_hash":"abc123"}],"exchanges":{}}"#,
        );
        let result = load_app_identifiers(&path).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "MyApp");
        assert_eq!(result[0].topic, "apps.myapp");
        assert_eq!(result[0].contract_hash, "abc123");
    }

    #[test]
    fn load_app_identifiers_returns_err_for_missing_file() {
        assert!(load_app_identifiers("/nonexistent/apps.json").is_err());
    }

    #[test]
    fn load_app_identifiers_returns_err_for_invalid_json() {
        let path = write_temp("test_apps_invalid.json", "not json");
        assert!(load_app_identifiers(&path).is_err());
    }

    #[test]
    fn normalize_hash_strips_prefix() {
        assert_eq!(normalize_hash("hash-abc123"), "abc123");
        assert_eq!(normalize_hash("abc123"), "abc123");
    }

    #[test]
    fn identify_matches_known_contract_hash() {
        let identifier = PerAppIdentifier {
            name: "MyApp".to_string(),
            topic: "apps.myapp".to_string(),
            contract_hash: "abc123".to_string(),
        };

        let event = identifier
            .identify(&make_tx("tx1", Some("abc123"), Some("transfer")))
            .unwrap()
            .unwrap();

        assert_eq!(event.app_data["contract_hash"], "abc123");
        assert_eq!(event.app_data["contract_name"], "MyApp");
        assert_eq!(event.app_data["entry_point"], "transfer");
    }

    #[test]
    fn identify_returns_none_for_different_contract() {
        let identifier = PerAppIdentifier {
            name: "MyApp".to_string(),
            topic: "apps.myapp".to_string(),
            contract_hash: "abc123".to_string(),
        };

        assert!(identifier
            .identify(&make_tx("tx1", Some("other_hash"), None))
            .unwrap()
            .is_none());
    }

    #[test]
    fn identify_returns_none_when_no_contract_hash() {
        let identifier = PerAppIdentifier {
            name: "MyApp".to_string(),
            topic: "apps.myapp".to_string(),
            contract_hash: "abc123".to_string(),
        };

        assert!(identifier.identify(&make_tx("tx1", None, None)).unwrap().is_none());
    }

    #[test]
    fn identify_includes_args_in_app_data() {
        let identifier = PerAppIdentifier {
            name: "MyApp".to_string(),
            topic: "apps.myapp".to_string(),
            contract_hash: "abc123".to_string(),
        };

        let mut tx = make_tx("tx1", Some("abc123"), Some("mint"));
        tx.args = Some(serde_json::json!({"address": "account-hash-abc", "amount": "1000"}));

        let event = identifier.identify(&tx).unwrap().unwrap();
        assert_eq!(event.app_data["args"]["amount"], "1000");
    }

    #[test]
    fn identify_omits_entry_point_when_absent() {
        let identifier = PerAppIdentifier {
            name: "MyApp".to_string(),
            topic: "apps.myapp".to_string(),
            contract_hash: "abc123".to_string(),
        };

        let event = identifier
            .identify(&make_tx("tx1", Some("abc123"), None))
            .unwrap()
            .unwrap();

        assert!(event.app_data.get("entry_point").is_none());
    }
}
