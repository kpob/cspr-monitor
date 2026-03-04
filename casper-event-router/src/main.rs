use anyhow::{Context, Result};
use casper_common::{
    Database, ENRICHED_CHAIN_EVENTS, KafkaConsumer, KafkaMessage, KafkaProducer, PostgresDB, RAW_CHAIN_EVENTS, RawEvent
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

mod correlator;
mod identifiers;

use correlator::TransactionCorrelator;
use identifiers::IdentifierRegistry;

const MAX_CONCURRENCY: usize = 4;
const CLEANUP_INTERVAL_SECS: u64 = 60;
const CORRELATION_TIMEOUT_SECS: u64 = 300; // 5 minutes

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    casper_common::init_tracing();

    tracing::info!("Starting casper-event-router service");

    // Initialize dependencies
    let db = Arc::new(PostgresDB::new().await?);
    let kafka_consumer = KafkaConsumer::new("event-router", vec![RAW_CHAIN_EVENTS])
        .await
        .context("Failed to create Kafka consumer")?;
    let kafka_producer = Arc::new(
        KafkaProducer::new()
            .await
            .context("Failed to create Kafka producer")?,
    );

    // Create correlator and identifier registry
    let correlator = Arc::new(TransactionCorrelator::new(Duration::from_secs(
        CORRELATION_TIMEOUT_SECS,
    )));
    let identifiers = Arc::new(IdentifierRegistry::new());

    // Spawn background cleanup task
    {
        let correlator = Arc::clone(&correlator);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(CLEANUP_INTERVAL_SECS));
            loop {
                interval.tick().await;
                let now = chrono::Utc::now();
                correlator.cleanup_expired(now);
                let stats = correlator.stats();
                tracing::info!(
                    "Correlator stats: {} pending accepted, {} pending processed",
                    stats.pending_accepted,
                    stats.pending_processed
                );
            }
        });
    }

    // Spawn background task to hot-reload identifier configs (contracts/exchanges files
    // may not exist at startup if the simulator hasn't deployed yet)
    {
        let identifiers = Arc::clone(&identifiers);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                identifiers.reload_all();
            }
        });
    }

    // Main event processing loop
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENCY));

    tracing::info!("Event router ready, waiting for messages...");

    loop {
        // Poll for message with timeout
        let message = match kafka_consumer.poll(Duration::from_secs(5)).await {
            Ok(Some(msg)) => msg,
            Ok(None) => continue, // Timeout, try again
            Err(e) => {
                tracing::error!("Error polling Kafka: {}", e);
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        // Acquire semaphore permit for concurrency control
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let db = Arc::clone(&db);
        let kafka_producer = Arc::clone(&kafka_producer);
        let correlator = Arc::clone(&correlator);
        let identifiers = Arc::clone(&identifiers);

        // Spawn task to process the event
        tokio::spawn(async move {
            let _permit = permit;

            if let Err(e) = process_event(
                message,
                &db,
                &kafka_producer,
                &correlator,
                &identifiers,
            )
            .await
            {
                tracing::error!("Error processing event: {:#}", e);
            }
        });

        // Commit offset after spawning the task
        if let Err(e) = kafka_consumer.commit() {
            tracing::error!("Failed to commit offset: {}", e);
        }
    }
}

async fn process_event(
    message: KafkaMessage,
    db: &PostgresDB,
    kafka_producer: &KafkaProducer,
    correlator: &TransactionCorrelator,
    identifiers: &IdentifierRegistry,
) -> Result<()> {
    // Deserialize the raw event
    let raw_event = RawEvent::try_from(message)?;

    tracing::debug!(
        "Processing event: id={}, type={}",
        raw_event.id,
        raw_event.event_type
    );

    // Attempt to correlate the event
    let enriched_tx = match correlator.correlate(&raw_event)? {
        Some(tx) => tx,
        None => {
            // Still waiting for matching event
            tracing::debug!("Waiting for matching event for: {}", raw_event.id);
            return Ok(());
        }
    };

    tracing::info!(
        "Correlated transaction: {} (accepted at: {}, processed at: {}, status: {})",
        enriched_tx.tx_hash,
        enriched_tx.accepted_at,
        enriched_tx.processed_at,
        enriched_tx.status
    );

    // Write to database for queryability (single write for both accepted and processed)
    db.insert_transaction_lifecycle(
        &enriched_tx.tx_hash,
        &enriched_tx.accepted_at.to_string(),
        &enriched_tx.sender,
        enriched_tx.raw_accepted.clone(),
        &enriched_tx.processed_at.to_string(),
        &enriched_tx.status,
        enriched_tx.raw_processed.clone(),
    )
    .await?;

    // Publish to enriched.chain_events topic (all enriched transactions)
    kafka_producer
        .publish_json(
            ENRICHED_CHAIN_EVENTS,
            &enriched_tx.tx_hash,
            &enriched_tx,
        )
        .await
        .context("Failed to publish to enriched.chain_events")?;

    tracing::debug!("Published to {}: {}", ENRICHED_CHAIN_EVENTS, enriched_tx.tx_hash);

    // Run app identifiers
    let app_events = identifiers
        .identify_all(&enriched_tx)
        .context("Failed to run app identifiers")?;

    // Publish app-specific events to their respective topics
    for (topic, app_event) in app_events {
        kafka_producer
            .publish_json(topic, &app_event.tx_hash, &app_event)
            .await
            .with_context(|| format!("Failed to publish to topic {}", topic))?;

        tracing::info!(
            "Published app event: topic={}, tx_hash={}",
            topic,
            app_event.tx_hash
        );
    }

    Ok(())
}
