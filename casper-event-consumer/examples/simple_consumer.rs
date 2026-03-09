use anyhow::Result;
use casper_common::{APPS_EXCHANGES, APPS_UNCLASSIFIED};
use casper_event_consumer::{EnrichedEvent, EventConsumer, EventHandler};

/// A simple event handler that just prints events
struct SimpleHandler;

#[async_trait::async_trait]
impl EventHandler for SimpleHandler {
    async fn handle(&self, event: EnrichedEvent) -> Result<()> {
        println!("========================================");
        println!("Received Event:");
        println!("  TX Hash: {}", event.tx_hash);
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

    let brokers = std::env::var("KAFKA_BROKERS").unwrap_or_else(|_| "localhost:9092".to_string());

    // Create consumer
    let consumer = EventConsumer::builder()
        .brokers(brokers)
        .topics(vec![APPS_EXCHANGES, APPS_UNCLASSIFIED])
        .group_id("simple-consumer-example")
        .build()?;

    // Subscribe with handler
    let handler = SimpleHandler;
    consumer.subscribe(handler).await?;

    Ok(())
}
