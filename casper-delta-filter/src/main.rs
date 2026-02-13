use anyhow::Result;
use casper_event_consumer::{EnrichedEvent, EventConsumer, EventHandler};

const CASPER_DELTA_CONTRACT_HASH: &str =
    "4b15cdbc606589ebfdcdb3f8f81e2a2a0d44298e3d1faf0743d2be2d90d8cb6b";

/// Handler for Delta contract events
struct DeltaFilter;

#[async_trait::async_trait]
impl EventHandler for DeltaFilter {
    async fn handle(&self, event: EnrichedEvent) -> Result<()> {
        // Check if this is a contract interaction event
        if event.app_type != "contract_interaction" {
            return Ok(());
        }

        // Extract contract hash from app_data
        let contract_hash = event
            .app_data
            .get("contract_hash")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Check if this is a Delta contract interaction
        if contract_hash == CASPER_DELTA_CONTRACT_HASH {
            tracing::info!(
                "Delta contract interaction detected: tx_hash={}, status={}, sender={}",
                event.tx_hash,
                event.lifecycle.status,
                event.lifecycle.sender
            );

            // Extract entry point if available
            if let Some(entry_point) = event.app_data.get("entry_point") {
                tracing::info!("  Entry point: {}", entry_point);
            }

            // Log full app data for debugging
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
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    casper_common::init_tracing();

    tracing::info!("Starting casper-delta-filter service");
    tracing::info!("Monitoring Delta contract: {}", CASPER_DELTA_CONTRACT_HASH);

    // Create consumer for contract events
    let consumer = EventConsumer::builder()
        .brokers(std::env::var("KAFKA_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".to_string()))
        .topics(vec!["apps.contracts"])
        .group_id("delta-filter-v1")
        .build()?;

    // Subscribe with handler
    let handler = DeltaFilter;
    consumer.subscribe(handler).await?;

    Ok(())
}
