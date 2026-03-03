use anyhow::Result;
use casper_event_consumer::{EnrichedEvent, EventConsumer, EventHandler};

/// Handler that logs exchange inflow and outflow events.
///
/// Set `EXCHANGE_FILTER=<name>` to show only events for a specific exchange.
struct ExchangeMonitor {
    filter_exchange: Option<String>,
}

#[async_trait::async_trait]
impl EventHandler for ExchangeMonitor {
    async fn handle(&self, event: EnrichedEvent) -> Result<()> {
        // Only process exchange activity events
        if event.app_type != "exchange_activity" {
            return Ok(());
        }

        let exchange = event.app_data["exchange"].as_str().unwrap_or("Unknown");

        // Optional per-exchange filter
        if let Some(filter) = &self.filter_exchange {
            if exchange != filter {
                return Ok(());
            }
        }

        let direction = event.app_data["direction"].as_str().unwrap_or("unknown");
        let counterparty = event.app_data["counterparty"].as_str().unwrap_or("unknown");
        let amount = event
            .app_data
            .get("amount")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let preposition = if direction == "inflow" { "from" } else { "to" };

        tracing::info!(
            "[{}] {} {} motes {} {} (tx={}, status={})",
            exchange,
            direction.to_uppercase(),
            amount,
            preposition,
            counterparty,
            &event.tx_hash[..event.tx_hash.len().min(12)],
            event.lifecycle.status,
        );

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    casper_common::init_tracing();

    tracing::info!("Starting casper-exchange-monitor");

    let filter_exchange = std::env::var("EXCHANGE_FILTER").ok();
    match &filter_exchange {
        Some(name) => tracing::info!("Filtering for exchange: {}", name),
        None => tracing::info!("Monitoring all exchanges (set EXCHANGE_FILTER=<name> to narrow)"),
    }

    let consumer = EventConsumer::builder()
        .brokers(std::env::var("KAFKA_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".to_string()))
        .topics(vec!["apps.exchanges"])
        .group_id("exchange-monitor-v1")
        .build()?;

    consumer.subscribe(ExchangeMonitor { filter_exchange }).await?;

    Ok(())
}
