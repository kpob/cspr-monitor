use anyhow::{Context, Result};
use casper_common::{EnrichedTransaction, RawEvent, TRANSACTION_ACCEPTED, TRANSACTION_PROCESSED};
use casper_types::{
    execution::ExecutionResult, ExecutableDeployItem, Transaction, TransactionEntryPoint,
    TransactionHash, TransactionInvocationTarget, TransactionTarget,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use std::time::Duration;

#[derive(Debug, Clone)]
struct AcceptedEvent {
    tx_hash: String,
    sender: String,
    accepted_at: DateTime<Utc>,
    raw_accepted: serde_json::Value,
    contract_hash: Option<String>,
    entry_point: Option<String>,
    args: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
struct ProcessedEvent {
    tx_hash: String,
    status: String,
    processed_at: DateTime<Utc>,
    raw_processed: serde_json::Value,
}

/// Correlates TransactionAccepted and TransactionProcessed events by tx_hash
pub struct TransactionCorrelator {
    pending_accepted: DashMap<String, AcceptedEvent>,
    pending_processed: DashMap<String, ProcessedEvent>,
    #[allow(dead_code)]
    timeout: Duration,
}

impl TransactionCorrelator {
    pub fn new(timeout: Duration) -> Self {
        Self {
            pending_accepted: DashMap::new(),
            pending_processed: DashMap::new(),
            timeout,
        }
    }

    /// Attempt to correlate a raw event
    ///
    /// Returns Some(EnrichedTransaction) if both accepted and processed events are available
    /// Returns None if we're still waiting for the matching event
    pub fn correlate(&self, event: &RawEvent) -> Result<Option<EnrichedTransaction>> {
        match event.event_type.as_str() {
            TRANSACTION_ACCEPTED => self.handle_accepted(event),
            TRANSACTION_PROCESSED => self.handle_processed(event),
            _ => Ok(None),
        }
    }

    fn handle_accepted(&self, event: &RawEvent) -> Result<Option<EnrichedTransaction>> {
        // Parse the TransactionAccepted event
        let transaction = serde_json::from_value::<Transaction>(
            event.payload["TransactionAccepted"].clone(),
        )
        .context("Failed to parse Transaction from TransactionAccepted event")?;

        let tx_hash = transaction.hash().to_hex_string();
        let sender = match transaction.initiator_addr() {
            casper_types::InitiatorAddr::PublicKey(pk) => pk.to_hex_string(),
            casper_types::InitiatorAddr::AccountHash(ah) => ah.to_hex_string(),
        };

        let ta_value = &event.payload["TransactionAccepted"];
        let (contract_hash, entry_point, args) = if let Some(v1) = ta_value.get("Version1") {
            extract_from_v1(&v1["payload"])
        } else if let Some(deploy) = ta_value.get("Deploy") {
            extract_from_deploy(deploy)
        } else {
            (None, None, None)
        };

        let accepted_event = AcceptedEvent {
            tx_hash: tx_hash.clone(),
            sender,
            accepted_at: event.received_at,
            raw_accepted: event.payload.clone(),
            contract_hash,
            entry_point,
            args,
        };

        // Check if we already have the ProcessedEvent
        if let Some((_, processed)) = self.pending_processed.remove(&tx_hash) {
            // We have both! Merge and return
            Ok(Some(Self::merge(accepted_event, processed)))
        } else {
            // Store the AcceptedEvent and wait for ProcessedEvent
            self.pending_accepted.insert(tx_hash.clone(), accepted_event);
            tracing::debug!("Stored pending accepted event for tx_hash: {}", tx_hash);
            Ok(None)
        }
    }

    fn handle_processed(&self, event: &RawEvent) -> Result<Option<EnrichedTransaction>> {
        // Parse the TransactionProcessed event
        let data = &event.payload["TransactionProcessed"];
        let execution_result = serde_json::from_value::<ExecutionResult>(
            data["execution_result"].clone(),
        )
        .context("Failed to parse ExecutionResult from TransactionProcessed event")?;

        let tx_hash = serde_json::from_value::<TransactionHash>(
            data["transaction_hash"].clone(),
        )
        .context("Failed to parse TransactionHash from TransactionProcessed event")?
        .to_hex_string();

        let timestamp = data["timestamp"]
            .as_str()
            .context("Missing timestamp in TransactionProcessed event")?;
        let processed_at = timestamp
            .parse::<DateTime<Utc>>()
            .context("Failed to parse timestamp")?;

        let status = match execution_result.error_message() {
            Some(_) => "failure".to_string(),
            None => "success".to_string(),
        };

        let processed_event = ProcessedEvent {
            tx_hash: tx_hash.clone(),
            status,
            processed_at,
            raw_processed: event.payload.clone(),
        };

        // Check if we already have the AcceptedEvent
        if let Some((_, accepted)) = self.pending_accepted.remove(&tx_hash) {
            // We have both! Merge and return
            Ok(Some(Self::merge(accepted, processed_event)))
        } else {
            // Store the ProcessedEvent and wait for AcceptedEvent
            self.pending_processed.insert(tx_hash.clone(), processed_event);
            tracing::debug!("Stored pending processed event for tx_hash: {}", tx_hash);
            Ok(None)
        }
    }

    fn merge(accepted: AcceptedEvent, processed: ProcessedEvent) -> EnrichedTransaction {
        EnrichedTransaction {
            tx_hash: accepted.tx_hash,
            accepted_at: accepted.accepted_at,
            processed_at: processed.processed_at,
            sender: accepted.sender,
            status: processed.status,
            raw_accepted: accepted.raw_accepted,
            raw_processed: processed.raw_processed,
            contract_hash: accepted.contract_hash,
            entry_point: accepted.entry_point,
            args: accepted.args,
        }
    }

    /// Clean up expired pending events (those older than timeout)
    ///
    /// This should be called periodically in a background task
    pub fn cleanup_expired(&self, now: DateTime<Utc>) {
        let timeout_secs = self.timeout.as_secs() as i64;

        self.pending_accepted.retain(|_, event| {
            let age = now.signed_duration_since(event.accepted_at).num_seconds();
            if age > timeout_secs {
                tracing::warn!(
                    "Removing expired accepted event for tx_hash: {} (age: {}s)",
                    event.tx_hash,
                    age
                );
                false
            } else {
                true
            }
        });

        self.pending_processed.retain(|_, event| {
            let age = now.signed_duration_since(event.processed_at).num_seconds();
            if age > timeout_secs {
                tracing::warn!(
                    "Removing expired processed event for tx_hash: {} (age: {}s)",
                    event.tx_hash,
                    age
                );
                false
            } else {
                true
            }
        });
    }

    /// Get statistics about pending events
    pub fn stats(&self) -> CorrelatorStats {
        CorrelatorStats {
            pending_accepted: self.pending_accepted.len(),
            pending_processed: self.pending_processed.len(),
        }
    }
}

/// Extract contract hash, entry point, and args from a V1 transaction payload value.
///
/// `payload_value` is the JSON object at `TransactionAccepted.Version1.payload`.
fn extract_from_v1(payload_value: &serde_json::Value) -> (Option<String>, Option<String>, Option<serde_json::Value>) {
    let fields = payload_value.get("fields");

    // Deserialize the target field as a typed TransactionTarget and extract the hash.
    let contract_hash = fields
        .and_then(|f| f.get("target"))
        .and_then(|t| serde_json::from_value::<TransactionTarget>(t.clone()).ok())
        .and_then(|t| match t {
            TransactionTarget::Stored { id, .. } => match id {
                TransactionInvocationTarget::ByPackageHash { addr, .. } => {
                    Some(addr.iter().map(|b| format!("{b:02x}")).collect())
                }
                TransactionInvocationTarget::ByHash(h) => {
                    Some(h.iter().map(|b| format!("{b:02x}")).collect())
                }
                _ => None,
            },
            _ => None,
        });

    // The SSE stream represents entry points in two forms:
    //   - plain string for native variants: "Transfer", "AddBid", etc.
    //   - object for custom entry points:   {"Custom": "mint"}
    // We try serde_json deserialization first (handles both), then fall back to
    // TransactionEntryPoint::from() for the lowercase-string form used in tests.
    let entry_point = fields
        .and_then(|f| f.get("entry_point"))
        .and_then(|ep| {
            serde_json::from_value::<TransactionEntryPoint>(ep.clone())
                .ok()
                .or_else(|| ep.as_str().map(TransactionEntryPoint::from))
        })
        .map(|ep| match ep {
            TransactionEntryPoint::Custom(name) => name,
            other => other.to_string(),
        });

    // Args are in {"Named": [["name", {"parsed": ..., ...}], ...]} format.
    // Build a simple {name: parsed_value} object for easy consumption.
    let args = fields
        .and_then(|f| f.get("args"))
        .and_then(|a| a.get("Named"))
        .and_then(|named| named.as_array())
        .map(|arr| {
            let mut map = serde_json::Map::new();
            for item in arr {
                if let (Some(name), Some(parsed)) = (
                    item.get(0).and_then(|n| n.as_str()),
                    item.get(1).and_then(|v| v.get("parsed")),
                ) {
                    map.insert(name.to_string(), parsed.clone());
                }
            }
            serde_json::Value::Object(map)
        });

    maybe_unwrap_proxy_call(contract_hash, entry_point, args)
}

/// Extract contract hash, entry point, and args from a Deploy transaction value.
///
/// `deploy_value` is the JSON object at `TransactionAccepted.Deploy`.
/// Session is variant-wrapped: `{"Transfer": {"args": [...]}}`,
/// `{"StoredContractByHash": {"hash": "...", "entry_point": "...", "args": [...]}}`, etc.
fn extract_from_deploy(deploy_value: &serde_json::Value) -> (Option<String>, Option<String>, Option<serde_json::Value>) {
    let session = deploy_value
        .get("session")
        .and_then(|s| serde_json::from_value::<ExecutableDeployItem>(s.clone()).ok());

    let contract_hash = session.as_ref().and_then(|s| match s {
        ExecutableDeployItem::StoredContractByHash { hash, .. } => Some(format!("{hash}")),
        ExecutableDeployItem::StoredVersionedContractByHash { hash, .. } => {
            Some(format!("{hash}"))
        }
        _ => None,
    });

    let entry_point = session
        .as_ref()
        .map(|s| s.entry_point_name().to_string())
        .filter(|s| !s.is_empty());

    // Deploy session JSON is variant-wrapped: {"Transfer": {"args": [...]}} or
    // {"StoredContractByHash": {"hash": "...", "args": [...]}}.
    // Skip the variant key by taking the first (and only) value in the session object.
    let args = deploy_value
        .get("session")
        .and_then(|s| s.as_object())
        .and_then(|obj| obj.values().next())
        .and_then(|inner| inner.get("args"))
        .and_then(|a| a.as_array())
        .map(|arr| {
            let mut map = serde_json::Map::new();
            for item in arr {
                if let (Some(name), Some(parsed)) = (
                    item.get(0).and_then(|n| n.as_str()),
                    item.get(1).and_then(|v| v.get("parsed")),
                ) {
                    map.insert(name.to_string(), parsed.clone());
                }
            }
            serde_json::Value::Object(map)
        });

    maybe_unwrap_proxy_call(contract_hash, entry_point, args)
}

/// If `args` contains Odra proxy-caller fields (`package_hash`, `entry_point`), treat the
/// transaction as a contract call by promoting the package hash to `contract_hash` and the
/// inner entry point name to `entry_point`.
///
/// Odra's proxy-caller WASM is deployed as a Session (no stored contract target), so without
/// this step the transaction would have no `contract_hash` and be silently classified as a
/// plain WASM session.  The serialised inner `RuntimeArgs` bytes are decoded into the same
/// `{name: parsed_value}` map that regular contract calls produce, so downstream consumers
/// see a uniform args format regardless of whether the call was direct or via proxy.
fn maybe_unwrap_proxy_call(
    contract_hash: Option<String>,
    entry_point: Option<String>,
    args: Option<serde_json::Value>,
) -> (Option<String>, Option<String>, Option<serde_json::Value>) {
    // Only rewrite session/WASM transactions (those without a resolved contract hash).
    if contract_hash.is_some() {
        return (contract_hash, entry_point, args);
    }

    let args_map = match &args {
        Some(serde_json::Value::Object(m)) => m,
        _ => return (contract_hash, entry_point, args),
    };

    let proxy_hash = args_map
        .get("package_hash")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    if let Some(hash) = proxy_hash {
        let inner_ep = args_map
            .get("entry_point")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or(entry_point);

        // Decode the serialised inner RuntimeArgs into a flat {name: parsed_value} map.
        // Falls back to the original outer args map if decoding fails.
        let decoded = decode_proxy_inner_args(args_map);
        (Some(hash), inner_ep, decoded.or(args))
    } else {
        (contract_hash, entry_point, args)
    }
}

/// Decode the Odra proxy-caller's serialised `RuntimeArgs` bytes (the `"args"` field in the
/// outer named-args map) into a `{name: parsed_value}` JSON object.
///
/// The `"attached_value"` field from the outer map is included in the result so consumers can
/// see how much CSPR was attached to the call without digging into the raw payload.
///
/// Returns `None` if the bytes field is absent, not valid hex, or cannot be parsed as
/// `RuntimeArgs` — the caller falls back to the unmodified outer args in that case.
fn decode_proxy_inner_args(
    outer: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    use casper_types::RuntimeArgs;
    use casper_types::bytesrepr::FromBytes;

    let bytes = parse_args_bytes(outer.get("args")?)?;
    let (runtime_args, _) = RuntimeArgs::from_bytes(&bytes).ok()?;

    let mut map = serde_json::Map::new();
    for named_arg in runtime_args.named_args() {
        // CLValue serialises as {"cl_type":…,"bytes":"…","parsed":…} with json-schema feature.
        let cl_json = serde_json::to_value(named_arg.cl_value()).ok()?;
        let parsed = cl_json
            .get("parsed")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        map.insert(named_arg.name().to_string(), parsed);
    }

    // Carry attached_value through so consumers know how much CSPR was attached.
    if let Some(val) = outer.get("attached_value") {
        map.insert("attached_value".to_string(), val.clone());
    }

    Some(serde_json::Value::Object(map))
}

/// Convert the `parsed` value of a Casper CLValue carrying raw bytes into a `Vec<u8>`.
///
/// The Casper SSE stream represents bytes in two ways depending on CLType:
/// - `List(U8)` (i.e. `Bytes`) → JSON array of u8 integers: `[5, 0, 0, …]`
/// - `ByteArray(n)`            → lowercase hex string: `"0102abcd…"`
fn parse_args_bytes(val: &serde_json::Value) -> Option<Vec<u8>> {
    match val {
        serde_json::Value::Array(arr) => arr
            .iter()
            .map(|v| v.as_u64().and_then(|n| u8::try_from(n).ok()))
            .collect(),
        serde_json::Value::String(s) => {
            if s.len() % 2 != 0 {
                return None;
            }
            (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
                .collect()
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_from_deploy_transfer_parses_args() {
        // Deploy-format native transfer: session is variant-wrapped as {"Transfer": {"args": [...]}}
        let deploy_value = serde_json::json!({
            "session": {
                "Transfer": {
                    "args": [
                        ["target", {"cl_type": "Key", "bytes": "", "parsed": "account-hash-abc123"}],
                        ["amount", {"cl_type": "U512", "bytes": "", "parsed": "5000000000"}],
                        ["id", {"cl_type": "Option (U64)", "bytes": "", "parsed": null}]
                    ]
                }
            }
        });
        let (contract_hash, _entry_point, args) = extract_from_deploy(&deploy_value);
        assert!(contract_hash.is_none());
        let args = args.expect("args should be extracted from Transfer deploy");
        assert_eq!(args["target"], "account-hash-abc123");
        assert_eq!(args["amount"], "5000000000");
    }

    #[test]
    fn extract_from_deploy_stored_contract_parses_hash_and_args() {
        let deploy_value = serde_json::json!({
            "session": {
                "StoredContractByHash": {
                    "hash": "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20",
                    "entry_point": "transfer",
                    "args": [
                        ["amount", {"cl_type": "U256", "bytes": "", "parsed": "1000"}]
                    ]
                }
            }
        });
        let (contract_hash, entry_point, args) = extract_from_deploy(&deploy_value);
        assert!(contract_hash.is_some());
        assert_eq!(entry_point.as_deref(), Some("transfer"));
        let args = args.expect("args should be extracted");
        assert_eq!(args["amount"], "1000");
    }

    #[test]
    fn proxy_call_wasm_session_promotes_package_hash_as_contract_hash() {
        use casper_types::RuntimeArgs;
        use casper_types::bytesrepr::ToBytes;

        // Build a real RuntimeArgs with one arg so the bytes round-trip properly.
        let mut inner = RuntimeArgs::new();
        inner.insert("amount", casper_types::U256::from(1000u64)).unwrap();
        // Represent bytes as a JSON array of integers (List(U8) / Bytes CLType format from SSE).
        let inner_bytes = inner.to_bytes().unwrap();
        let inner_arr: Vec<serde_json::Value> = inner_bytes
            .iter()
            .map(|b| serde_json::json!(*b))
            .collect();

        let (contract_hash, entry_point, args) = maybe_unwrap_proxy_call(
            None,
            Some("call".to_string()),
            Some(serde_json::json!({
                "package_hash": "aabbccdd",
                "entry_point": "transfer",
                "args": inner_arr,
                "attached_value": "0"
            })),
        );
        assert_eq!(contract_hash.as_deref(), Some("aabbccdd"));
        assert_eq!(entry_point.as_deref(), Some("transfer"));
        // Inner args decoded: "amount" should appear as parsed value, "attached_value" carried through.
        let args = args.unwrap();
        assert!(args.get("amount").is_some(), "decoded inner arg 'amount' missing");
        assert_eq!(args["attached_value"], "0");
        // Outer proxy fields must NOT leak into the decoded args.
        assert!(args.get("package_hash").is_none());
        assert!(args.get("entry_point").is_none());
    }

    #[test]
    fn proxy_call_does_not_overwrite_existing_contract_hash() {
        let (contract_hash, entry_point, _) = maybe_unwrap_proxy_call(
            Some("existing_hash".to_string()),
            Some("mint".to_string()),
            Some(serde_json::json!({"package_hash": "other"})),
        );
        assert_eq!(contract_hash.as_deref(), Some("existing_hash"));
        assert_eq!(entry_point.as_deref(), Some("mint"));
    }

    #[test]
    fn proxy_call_no_op_when_package_hash_absent() {
        let (contract_hash, entry_point, args) = maybe_unwrap_proxy_call(
            None,
            Some("call".to_string()),
            Some(serde_json::json!({"amount": "1000"})),
        );
        assert!(contract_hash.is_none());
        assert_eq!(entry_point.as_deref(), Some("call"));
        // Original args untouched.
        assert_eq!(args.unwrap()["amount"], "1000");
    }

    #[test]
    fn proxy_call_falls_back_to_outer_args_when_inner_bytes_invalid() {
        // "args" is not valid hex → decode fails → keep outer args.
        let (contract_hash, _entry_point, args) = maybe_unwrap_proxy_call(
            None,
            Some("call".to_string()),
            Some(serde_json::json!({
                "package_hash": "aabbccdd",
                "entry_point": "transfer",
                "args": "not-hex!",
                "attached_value": "0"
            })),
        );
        assert_eq!(contract_hash.as_deref(), Some("aabbccdd"));
        // Falls back to original outer args.
        let args = args.unwrap();
        assert_eq!(args["args"], "not-hex!");
    }


    /// Full round-trip against a real SSE payload (res/tx.json).
    ///
    /// The transaction is a V1 WASM session using the Odra proxy-caller pattern.
    /// The outer args carry `package_hash` + `entry_point` + serialised inner `RuntimeArgs`
    /// (List(U8) / Bytes CLType).  After extraction:
    ///   - `contract_hash` must be promoted from the outer `package_hash` arg.
    ///   - `entry_point` must be the inner method name, not the WASM `"Call"` entry point.
    ///   - `args` must contain the decoded inner RuntimeArgs (5 args for this swap call)
    ///     and carry `attached_value` through; outer proxy fields must not leak.
    #[test]
    fn extract_from_v1_decodes_odra_proxy_call_from_real_payload() {
        let data = std::fs::read_to_string("./res/tx.json").expect("Failed to read res/tx.json");
        let json: serde_json::Value = serde_json::from_str(&data).expect("Invalid JSON");
        let payload = &json["Version1"]["payload"];

        let (contract_hash, entry_point, args) = extract_from_v1(payload);

        // Contract hash comes from the outer "package_hash" arg, not from the Session target.
        assert_eq!(
            contract_hash.as_deref(),
            Some("1dbac65585475fec53e5b1f9110923c8d232921702097e83105b36751d682186"),
        );

        // Entry point is the inner method, not the WASM "Call" entry point.
        assert_eq!(entry_point.as_deref(), Some("swap_tokens_for_exact_tokens"));

        let args = args.expect("args must be decoded");

        // Inner RuntimeArgs decoded: first arg is "amount_out".
        assert!(args.get("amount_out").is_some(), "expected inner arg 'amount_out'");

        // attached_value is carried from the outer proxy args.
        assert!(args.get("attached_value").is_some(), "expected 'attached_value'");

        // Outer proxy routing fields must not bleed into the decoded args.
        assert!(args.get("package_hash").is_none(), "package_hash must not leak into args");
        assert!(args.get("entry_point").is_none(), "entry_point must not leak into args");
    }
}

#[derive(Debug, Clone)]
pub struct CorrelatorStats {
    pub pending_accepted: usize,
    pub pending_processed: usize,
}
