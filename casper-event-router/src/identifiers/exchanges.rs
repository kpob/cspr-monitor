use anyhow::Result;
use casper_common::{AppEvent, EnrichedTransaction, TransactionLifecycle};
use chrono::Utc;
use std::collections::HashMap;

use super::AppIdentifier;

/// Identifies transactions from/to known exchange wallets
pub struct ExchangeWalletIdentifier {
    exchange_addresses: HashMap<String, String>,
}

impl ExchangeWalletIdentifier {
    pub fn new() -> Self {
        let mut exchange_addresses = HashMap::new();

        // Load from environment if available
        if let Ok(addresses) = std::env::var("EXCHANGE_ADDRESSES") {
            for addr in addresses.split(',') {
                let parts: Vec<&str> = addr.split('=').collect();
                if parts.len() == 2 {
                    exchange_addresses.insert(
                        parts[0].trim().to_string(),
                        parts[1].trim().to_string(),
                    );
                }
            }
        }

        // Add default known exchanges (from casper-delta-filter)
        if exchange_addresses.is_empty() {
            exchange_addresses.insert(
                "011c74ebfcc1b19bc3e578bec3ecfa2d484f2a00d7e9e8152c4c70f519f6a89f6a".to_string(),
                "Crypto.com".to_string(),
            );
            exchange_addresses.insert(
                "020396133b3bbbfcf7d1961390f9449e2de5813523180376df361cb31a1ca965b576".to_string(),
                "Crypto.com 2".to_string(),
            );
            exchange_addresses.insert(
                "013beea64514ca478b9c2f6c43089ef9f1d0de0e803893a40cc34d75509ec9e2d4".to_string(),
                "Bybit".to_string(),
            );
            exchange_addresses.insert(
                "016470ae57b0a3ad5a679d2e0422909bfb9ded445e20cbe6b4c9806f844c94d401".to_string(),
                "MEXC".to_string(),
            );
            exchange_addresses.insert(
                "0202d219ef1f971118e33eacbad3d01c66cd07abc59d269e28253189dab15232479d".to_string(),
                "KuCoin Cold 2".to_string(),
            );
            exchange_addresses.insert(
                "0202ed20f3a93b5386bc41b6945722b2bd4250c48f5fa0632adf546e2f3ff6f4ddee".to_string(),
                "KuCoin".to_string(),
            );
            exchange_addresses.insert(
                "01b92e36567350dd7b339d709bfe341df6fda853e85315418f1bb3ddd414d9f5be".to_string(),
                "Huobi".to_string(),
            );
            exchange_addresses.insert(
                "02029d865f743f9a67c82c84d443cbd8187bc4a08ca7b4c985f0caca1a4ee98b1f4c".to_string(),
                "Coinlist".to_string(),
            );
            exchange_addresses.insert(
                "02024c5e3ba7b1da49cda950319aec914cd3c720fbec3dcf25aa4add631e28f70aa9".to_string(),
                "CA".to_string(),
            );
            exchange_addresses.insert(
                "0202d39fac363e1d8f0101edde806863668eb87d21a591f00b724ef83d371c1fc696".to_string(),
                "Make".to_string(),
            );
            exchange_addresses.insert(
                "01a0d23e084a95cdee9c2fb226d54033d645873a7c7c9739de2158725c7dfe672f".to_string(),
                "Uphold".to_string(),
            );
        }

        tracing::info!(
            "Exchange wallet identifier initialized with {} exchanges",
            exchange_addresses.len()
        );

        Self { exchange_addresses }
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
        "apps.exchanges"
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

impl Default for ExchangeWalletIdentifier {
    fn default() -> Self {
        Self::new()
    }
}
