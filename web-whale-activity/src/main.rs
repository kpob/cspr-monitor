use anyhow::Result;
use web_dashboard_common::{Dashboard, EventMapper, EventRecord, run_dashboard, utils::shorten_address};

mod utils;

/// 100,000 CSPR in motes — anything below is dust.
const WHALE_THRESHOLD_MOTES: u64 = 100_000 * 1_000_000_000;

struct WhaleMapper;

impl EventMapper for WhaleMapper {
    fn map(&self, event: &casper_event_consumer::EnrichedEvent) -> Option<EventRecord> {
        let sender = event.lifecycle.sender.clone();

        let (action, amount, target) = if let Some(tx_type) = event.app_data.get("transaction_type") {
            let action = tx_type.as_str().unwrap_or("unknown").to_string();
            let (amount, target) = extract_native_args(&action, &event.app_data["args"]);
            (action, amount, target)
        } else if event.app_data.get("contract_name").is_some() {
            let contract = event.app_data["contract_name"].as_str().unwrap_or("unknown");
            let action = format!("contract:{}", contract);
            let amount = utils::parse_amount(&event.app_data["args"]);
            let target = event.app_data["contract_hash"].as_str().unwrap_or("unknown").to_string();
            (action, amount, target)
        } else {
            ("unknown".to_string(), 0, "unknown".to_string())
        };

        if amount < WHALE_THRESHOLD_MOTES { return None; }

        Some(EventRecord {
            timestamp: event.lifecycle.processed_at.clone(),
            actor: shorten_address(&sender),
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
    let amount = utils::parse_amount(args);
    match action {
        "native_transfer" | "session" => {
            let target = args.get("target").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
            (amount, target)
        }
        "delegation" | "undelegation" => {
            let validator = args.get("validator").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
            (amount, validator)
        }
        "redelegation" => {
            let new_validator = args.get("new_validator").and_then(|v| v.as_str())
                .or_else(|| args.get("validator").and_then(|v| v.as_str()))
                .unwrap_or("unknown")
                .to_string();
            (amount, new_validator)
        }
        "add_bid" | "withdraw_bid" => (amount, "self".to_string()),
        _ => (amount, "unknown".to_string()),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let config_path = std::env::var("DASHBOARD_CONFIG")
        .unwrap_or_else(|_| "web-whale-activity/dashboard.toml".to_string());
    let dashboard = Dashboard::from_toml(&config_path, WhaleMapper)?;
    run_dashboard(dashboard).await
}
