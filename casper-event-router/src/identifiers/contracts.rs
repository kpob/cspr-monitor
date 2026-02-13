use anyhow::Result;
use casper_common::{AppEvent, EnrichedTransaction, TransactionLifecycle};
use chrono::Utc;
use std::collections::HashSet;

use super::AppIdentifier;

/// Identifies transactions targeting specific smart contracts
pub struct ContractPatternIdentifier {
    contract_hashes: HashSet<String>,
}

impl ContractPatternIdentifier {
    pub fn new() -> Self {
        // Load contract hashes from environment or use defaults
        let contract_patterns = std::env::var("CONTRACT_PATTERNS")
            .unwrap_or_else(|_| {
                // Default: Casper Delta contract
                "4b15cdbc606589ebfdcdb3f8f81e2a2a0d44298e3d1faf0743d2be2d90d8cb6b".to_string()
            });

        let contract_hashes: HashSet<String> = contract_patterns
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        tracing::info!(
            "Contract pattern identifier initialized with {} contracts",
            contract_hashes.len()
        );

        Self { contract_hashes }
    }

    fn extract_contract_hash(&self, tx: &EnrichedTransaction) -> Option<String> {
        // Try V1 transaction format
        if let Some(target) = tx.raw_accepted
            .get("TransactionAccepted")?
            .get("Version1")?
            .get("payload")?
            .get("fields")?
            .get("target")?
            .get("Stored")?
            .get("id")?
            .get("ByPackageHash")?
            .get("addr")
        {
            return target.as_str().map(|s| s.to_string());
        }

        // Try Deploy format
        if let Some(target) = tx.raw_accepted
            .get("TransactionAccepted")?
            .get("Deploy")?
            .get("session")?
            .get("StoredContractByHash")?
            .get("hash")
        {
            return target.as_str().map(|s| s.to_string());
        }

        None
    }
}

impl AppIdentifier for ContractPatternIdentifier {
    fn name(&self) -> &'static str {
        "contract_pattern"
    }

    fn topic(&self) -> &'static str {
        "apps.contracts"
    }

    fn identify(&self, tx: &EnrichedTransaction) -> Result<Option<AppEvent>> {
        // Extract contract hash from transaction
        let contract_hash = match self.extract_contract_hash(tx) {
            Some(hash) => hash,
            None => return Ok(None),
        };

        // Check if this contract is in our watch list
        if !self.contract_hashes.contains(&contract_hash) {
            return Ok(None);
        }

        // Build app-specific data
        let mut app_data = serde_json::Map::new();
        app_data.insert("contract_hash".to_string(), serde_json::json!(contract_hash));

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
