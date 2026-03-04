use anyhow::Result;
use casper_event_consumer::{EnrichedEvent, EventConsumer, EventHandler};

/// Handler for Casper Delta contract events
struct DeltaFilter;

#[async_trait::async_trait]
impl EventHandler for DeltaFilter {
    async fn handle(&self, event: EnrichedEvent) -> Result<()> {
        tracing::info!(
            "Delta contract interaction detected: tx_hash={}, status={}, sender={}",
            event.tx_hash,
            event.lifecycle.status,
            event.lifecycle.sender
        );

        if let Some(entry_point) = event.app_data.get("entry_point") {
            tracing::info!("  Entry point: {}", entry_point);
        }

        tracing::debug!(
            "  Full app_data: {}",
            serde_json::to_string_pretty(&event.app_data)?
        );

        // TODO: Add your Delta-specific processing logic here
        // For example:
        // - Analyze transfer amounts
        // - Track liquidity changes
        // - Detect arbitrage opportunities
        // - Send notifications
        // - Update external systems

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    casper_common::init_tracing();

    tracing::info!("Starting casper-delta-filter service");

    let consumer = EventConsumer::builder()
        .brokers(std::env::var("KAFKA_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".to_string()))
        .topics(vec!["apps.casper-delta"])
        .group_id("delta-filter-v1")
        .build()?;

    let handler = DeltaFilter;
    consumer.subscribe(handler).await?;

    Ok(())
}
