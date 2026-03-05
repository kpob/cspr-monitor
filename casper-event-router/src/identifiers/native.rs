use anyhow::Result;
use casper_common::{AppEvent, EnrichedTransaction, TransactionLifecycle};
use chrono::Utc;

/// Identifies native blockchain transactions: Transfer, Delegate, Undelegate, Redelegate,
/// AddBid, WithdrawBid, ActivateBid, and Session/WASM calls.
///
/// Fires for any enriched transaction with no contract hash (i.e., those not already
/// handled by per-app contract identifiers or the unclassified fallback). This covers
/// all transaction types that would otherwise be silently dropped after correlation.
pub struct NativeTransactionIdentifier;

impl NativeTransactionIdentifier {
    pub fn identify(&self, tx: &EnrichedTransaction) -> Result<Option<AppEvent>> {
        if tx.contract_hash.is_some() {
            return Ok(None);
        }

        let transaction_type = classify(tx.entry_point.as_deref());

        let mut app_data = serde_json::Map::new();
        app_data.insert(
            "transaction_type".to_string(),
            serde_json::json!(transaction_type),
        );
        if let Some(ep) = &tx.entry_point {
            app_data.insert("entry_point".to_string(), serde_json::json!(ep));
        }
        app_data.insert("sender".to_string(), serde_json::json!(&tx.sender));
        if let Some(args) = &tx.args {
            app_data.insert("args".to_string(), args.clone());
        }

        Ok(Some(AppEvent {
            event_id: format!("native-{}-{}", tx.tx_hash, Utc::now().timestamp_millis()),
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

fn classify(entry_point: Option<&str>) -> &'static str {
    match entry_point {
        Some("Transfer") => "native_transfer",
        Some("Delegate") => "delegation",
        Some("Undelegate") => "undelegation",
        Some("Redelegate") => "redelegation",
        Some("AddBid") => "add_bid",
        Some("WithdrawBid") => "withdraw_bid",
        Some("ActivateBid") => "activate_bid",
        _ => "session",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_tx(entry_point: Option<&str>, contract_hash: Option<&str>) -> EnrichedTransaction {
        EnrichedTransaction {
            tx_hash: "abc".to_string(),
            accepted_at: Utc::now(),
            processed_at: Utc::now(),
            sender: "sender-hash".to_string(),
            status: "success".to_string(),
            raw_accepted: serde_json::json!({}),
            raw_processed: serde_json::json!({}),
            contract_hash: contract_hash.map(str::to_string),
            entry_point: entry_point.map(str::to_string),
            args: None,
        }
    }

    #[test]
    fn skips_contract_transactions() {
        let id = NativeTransactionIdentifier;
        assert!(id.identify(&make_tx(Some("transfer"), Some("abc123"))).unwrap().is_none());
    }

    #[test]
    fn classifies_native_transfer() {
        let id = NativeTransactionIdentifier;
        let event = id.identify(&make_tx(Some("Transfer"), None)).unwrap().unwrap();
        assert_eq!(event.app_data["transaction_type"], "native_transfer");
    }

    #[test]
    fn classifies_delegation() {
        let id = NativeTransactionIdentifier;
        let event = id.identify(&make_tx(Some("Delegate"), None)).unwrap().unwrap();
        assert_eq!(event.app_data["transaction_type"], "delegation");
    }

    #[test]
    fn classifies_undelegation() {
        let id = NativeTransactionIdentifier;
        let event = id.identify(&make_tx(Some("Undelegate"), None)).unwrap().unwrap();
        assert_eq!(event.app_data["transaction_type"], "undelegation");
    }

    #[test]
    fn classifies_redelegation() {
        let id = NativeTransactionIdentifier;
        let event = id.identify(&make_tx(Some("Redelegate"), None)).unwrap().unwrap();
        assert_eq!(event.app_data["transaction_type"], "redelegation");
    }

    #[test]
    fn classifies_add_bid() {
        let id = NativeTransactionIdentifier;
        let event = id.identify(&make_tx(Some("AddBid"), None)).unwrap().unwrap();
        assert_eq!(event.app_data["transaction_type"], "add_bid");
    }

    #[test]
    fn classifies_withdraw_bid() {
        let id = NativeTransactionIdentifier;
        let event = id.identify(&make_tx(Some("WithdrawBid"), None)).unwrap().unwrap();
        assert_eq!(event.app_data["transaction_type"], "withdraw_bid");
    }

    #[test]
    fn classifies_activate_bid() {
        let id = NativeTransactionIdentifier;
        let event = id.identify(&make_tx(Some("ActivateBid"), None)).unwrap().unwrap();
        assert_eq!(event.app_data["transaction_type"], "activate_bid");
    }

    #[test]
    fn classifies_unknown_as_session() {
        let id = NativeTransactionIdentifier;
        let event = id.identify(&make_tx(None, None)).unwrap().unwrap();
        assert_eq!(event.app_data["transaction_type"], "session");
    }

    #[test]
    fn includes_entry_point_in_app_data() {
        let id = NativeTransactionIdentifier;
        let event = id.identify(&make_tx(Some("Delegate"), None)).unwrap().unwrap();
        assert_eq!(event.app_data["entry_point"], "Delegate");
        assert_eq!(event.app_data["sender"], "sender-hash");
    }
}
