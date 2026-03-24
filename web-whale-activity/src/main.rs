use anyhow::Result;
use web_dashboard_common::{DashboardConfig, EventMapper, EventRecord};

/// 100,000 CSPR in motes — anything below is dust.
const WHALE_THRESHOLD_MOTES: u64 = 100_000 * 1_000_000_000;

struct WhaleMapper;

impl EventMapper for WhaleMapper {
    fn map(&self, event: &casper_event_consumer::EnrichedEvent) -> Option<EventRecord> {
        let sender = event.lifecycle.sender.clone();

        // Determine action and extract target/amount based on topic schema.
        // apps.exchanges: has "exchange", "direction", "counterparty", "amount"
        // apps.native:    has "transaction_type", "sender", "args"
        // apps.contracts: has "contract_name", "contract_hash", "sender", "args"

        let (action, amount, target) = if let Some(tx_type) = event.app_data.get("transaction_type") {
            // apps.native event
            let action = tx_type.as_str().unwrap_or("unknown").to_string();
            let args = &event.app_data["args"];
            let (amount, target) = extract_native_args(&action, args);
            (action, amount, target)
        } else if event.app_data.get("exchange").is_some() {
            // apps.exchanges event
            let direction = event.app_data["direction"]
                .as_str()
                .unwrap_or("unknown");
            let action = if direction == "inflow" {
                "transfer_in".to_string()
            } else {
                "transfer_out".to_string()
            };
            let amount: u64 = event.app_data["amount"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let target = event.app_data["counterparty"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            (action, amount, target)
        } else if event.app_data.get("contract_name").is_some() {
            // apps.contracts event
            let contract = event.app_data["contract_name"]
                .as_str()
                .unwrap_or("unknown");
            let action = format!("contract:{}", contract);
            let args = &event.app_data["args"];
            let amount = parse_amount(args);
            let target = event.app_data["contract_hash"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            (action, amount, target)
        } else {
            ("unknown".to_string(), 0, "unknown".to_string())
        };

        // Filter out dust — only whale-sized transactions.
        if amount < WHALE_THRESHOLD_MOTES {
            return None;
        }

        let actor = shorten_address(&sender);

        Some(EventRecord {
            timestamp: event.lifecycle.processed_at.clone(),
            actor,
            actor_address: sender,
            action,
            amount,
            target,
            tx_hash: event.tx_hash.clone(),
            status: event.lifecycle.status.clone(),
        })
    }
}

fn extract_native_args(action: &str, args: &serde_json::Value) -> (u64, String) {
    match action {
        "native_transfer" | "sesssion" => {
            let amount = parse_amount(args);
            let target = args
                .get("target")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            (amount, target)
        }
        "delegation" | "undelegation" => {
            let amount = parse_amount(args);
            let validator = args
                .get("validator")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            (amount, validator)
        }
        "redelegation" => {
            let amount = parse_amount(args);
            let new_validator = args
                .get("new_validator")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| {
                    args.get("validator")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                })
                .to_string();
            (amount, new_validator)
        }
        "add_bid" | "withdraw_bid" => {
            let amount = parse_amount(args);
            (amount, "self".to_string())
        }
        _ => {
            let amount = parse_amount(args);
            (amount, "unknown".to_string())
        }
    }
}

fn parse_amount(args: &serde_json::Value) -> u64 {
    args.get("amount")
        .and_then(|v| v.as_str().or_else(|| v.as_u64().map(|_| "")).and_then(|s| {
            if s.is_empty() { v.as_u64() } else { s.parse().ok() }
        }))
        .unwrap_or(0)
}

fn shorten_address(addr: &str) -> String {
    if addr.len() > 16 {
        format!("{}..{}", &addr[..8], &addr[addr.len() - 6..])
    } else {
        addr.to_string()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    web_dashboard_common::run_dashboard(DashboardConfig {
        service_name: "web-whale-activity",
        web_port: 8081,
        prometheus_port: 9103,
        metric_name: "casper_whale_events_total",
        topics: vec!["apps.contracts", "apps.native"],
        group_id: "whale-activity-v1",
        dashboard_html: include_str!("dashboard.html"),
        mapper: WhaleMapper,
    })
    .await
}
