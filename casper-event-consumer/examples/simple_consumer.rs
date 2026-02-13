use anyhow::Result;
use casper_common::{APPS_CONTRACTS, APPS_EXCHANGES, ENRICHED_CHAIN_EVENTS};
use casper_event_consumer::{EnrichedEvent, EventConsumer, EventHandler};

/// A simple event handler that just prints events
struct SimpleHandler;

#[async_trait::async_trait]
impl EventHandler for SimpleHandler {
    async fn handle(&self, event: EnrichedEvent) -> Result<()> {
        println!("========================================");
        println!("Received Event:");
        println!("  TX Hash: {}", event.tx_hash);
        println!("  App Type: {}", event.app_type);
        println!("  Status: {}", event.lifecycle.status);
        println!("  Sender: {}", event.lifecycle.sender);
        println!("  Accepted At: {}", event.lifecycle.accepted_at);
        println!("  Processed At: {}", event.lifecycle.processed_at);
        println!("  App Data: {}", serde_json::to_string_pretty(&event.app_data)?);
        println!("========================================\n");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    casper_common::init_tracing();

    println!("Starting simple event consumer example...");
    println!("This will consume events from Kafka and print them to stdout.");
    println!("Press Ctrl+C to stop.\n");

    // Create consumer
    let consumer = EventConsumer::builder()
        .brokers("localhost:9092")
        .topics(vec![APPS_CONTRACTS, APPS_EXCHANGES, ENRICHED_CHAIN_EVENTS])
        .group_id("simple-consumer-example")
        .build()?;

    // Subscribe with handler
    let handler = SimpleHandler;
    consumer.subscribe(handler).await?;

    Ok(())
}
