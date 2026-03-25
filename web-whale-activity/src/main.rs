use std::{collections::HashMap, marker::PhantomData};

use anyhow::Result;
use axum::{Json, Router, extract::State, response::Html, routing::get};
use serde::Serialize;
use web_dashboard_common::{ApiRouter, AppState, DashboardConfig, EventMapper, EventRecord, UiRouter, utils::shorten_address};

mod utils;

/// 100,000 CSPR in motes — anything below is dust.
const WHALE_THRESHOLD_MOTES: u64 = 100_000 * 1_000_000_000;

#[derive(Serialize)]
struct AccountEventsResponse {
    address: String,
    events: Vec<EventRecord>,
    targets: HashMap<String, TargetSummary>,
}

#[derive(Serialize, Default)]
struct TargetSummary {
    tx_count: u64,
    total_amount: u64,
    actions: HashMap<String, u64>,
}

struct Routers;

impl UiRouter for Routers {
    fn ui() -> Router<AppState> {
        Router::new()
            .route("/", get(async || Html(include_str!("../html/dashboard.html"))))
            .route("/account/{address}", get(account_handler))
    }
}

impl ApiRouter for Routers {
    fn api() -> Router<AppState> {
        Self::base_api()
            .route("/api/{address}", get(account_events_handler))
    }
}

async fn account_handler(
    axum::extract::Path(_address): axum::extract::Path<String>,
) -> Html<&'static str> {
    Html(include_str!("../html/account.html"))
}

async fn account_events_handler(
    State(app): State<AppState>,
    axum::extract::Path(address): axum::extract::Path<String>,
) -> Json<AccountEventsResponse> {
    let guard = app.state.lock().await;
    let mut events = Vec::new();
    let mut targets: HashMap<String, TargetSummary> = HashMap::new();

    for record in guard.events.iter() {
        let is_sender = record.actor_address == address;
        let is_target = record.target == address;
        if !is_sender && !is_target {
            continue;
        }
        events.push(record.clone());
        let other = if is_sender {
            record.target.clone()
        } else {
            record.actor_address.clone()
        };
        let entry = targets.entry(other).or_default();
        entry.tx_count += 1;
        entry.total_amount += record.amount;
        *entry.actions.entry(record.action.clone()).or_default() += record.amount;
    }

    Json(AccountEventsResponse {
        address,
        events,
        targets,
    })
}

struct WhaleMapper;

impl EventMapper for WhaleMapper {
    fn map(&self, event: &casper_event_consumer::EnrichedEvent) -> Option<EventRecord> {
        let sender = event.lifecycle.sender.clone();

        // Determine action and extract target/amount based on topic schema.
        // apps.native:    has "transaction_type", "sender", "args"
        // apps.contracts: has "contract_name", "contract_hash", "sender", "args"
        let (action, amount, target) = if let Some(tx_type) = event.app_data.get("transaction_type") {
            // apps.native event
            let action = tx_type.as_str().unwrap_or("unknown").to_string();
            let args = &event.app_data["args"];
            let (amount, target) = extract_native_args(&action, args);
            (action, amount, target)
        } else if event.app_data.get("contract_name").is_some() {
            // apps.contracts event
            let contract = event.app_data["contract_name"]
                .as_str()
                .unwrap_or("unknown");
            let action = format!("contract:{}", contract);
            let args = &event.app_data["args"];
            let amount = utils::parse_amount(args);
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
    let amount = utils::parse_amount(args);
    match action {
        "native_transfer" | "session" => {
            let target = args
                .get("target")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            (amount, target)
        }
        "delegation" | "undelegation" => {
            let validator = args
                .get("validator")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            (amount, validator)
        }
        "redelegation" => {
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
        "add_bid" | "withdraw_bid" => (amount, "self".to_string()),
        _ => (amount, "unknown".to_string())
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
        mapper: WhaleMapper,
        _ui: PhantomData::<Routers>,
        _api: PhantomData::<Routers>,
    })
    .await
}
