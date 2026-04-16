use anyhow::Result;
use web_dashboard_common::{Dashboard, EventMapper, EventRecord, run_dashboard};

struct ExchangeMapper {
    filter: Option<String>,
}

impl EventMapper for ExchangeMapper {
    fn map(&self, event: &casper_event_consumer::EnrichedEvent) -> Option<EventRecord> {
        let exchange = event.app_data["exchange"].as_str().unwrap_or("Unknown").to_string();
        if let Some(f) = &self.filter {
            if exchange != *f { return None; }
        }

        let direction = event.app_data["direction"].as_str().unwrap_or("unknown").to_string();
        let counterparty = event.app_data["counterparty"].as_str().unwrap_or("unknown").to_string();
        let amount: u64 = event.app_data["amount"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        Some(EventRecord {
            timestamp: event.lifecycle.processed_at.clone(),
            actor: exchange,
            actor_address: event.lifecycle.sender.clone(),
            action: direction,
            amount,
            target: counterparty,
            tx_hash: event.tx_hash.clone(),
            status: event.lifecycle.status.clone(),
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mapper = ExchangeMapper { filter: std::env::var("EXCHANGE_FILTER").ok() };
    let config_path = std::env::var("DASHBOARD_CONFIG")
        .unwrap_or_else(|_| "casper-exchange-monitor/dashboard.toml".to_string());
    let dashboard = Dashboard::from_toml(&config_path, mapper)?;
    run_dashboard(dashboard).await
}
