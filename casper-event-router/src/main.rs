use anyhow::{Context, Result};
use casper_common::{
    Database, ENRICHED_CHAIN_EVENTS, KafkaConsumer, KafkaMessage, KafkaProducer, PostgresDB,
    RAW_CHAIN_EVENTS, RawEvent,
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
const DEFAULT_CONFIG_RELOAD_INTERVAL_SECS: u64 = 900;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    casper_common::init_tracing();

    tracing::info!("Starting casper-event-router service");

    casper_common::metrics::install_prometheus_exporter(9101);
    let health_tx =
        casper_common::health::spawn_health_server(9091, "casper-event-router");

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
                metrics::gauge!("casper_router_pending_accepted")
                    .set(stats.pending_accepted as f64);
                metrics::gauge!("casper_router_pending_processed")
                    .set(stats.pending_processed as f64);
                tracing::info!(
                    "Correlator stats: {} pending accepted, {} pending processed",
                    stats.pending_accepted,
                    stats.pending_processed
                );
            }
        });
    }

    // Spawn background task to hot-reload identifier configs
    {
        let identifiers = Arc::clone(&identifiers);
        let reload_interval_secs = std::env::var("CONFIG_RELOAD_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_CONFIG_RELOAD_INTERVAL_SECS);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(reload_interval_secs));
            loop {
                interval.tick().await;
                identifiers.reload_all();
            }
        });
    }

    // Spawn DB retention task
    {
        let db = Arc::clone(&db);
        tokio::spawn(async move {
            let days: i64 = std::env::var("RAW_EVENTS_RETENTION_DAYS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(7);
            if days == 0 {
                return;
            }
            let mut interval = tokio::time::interval(Duration::from_secs(86400));
            loop {
                interval.tick().await;
                match db.delete_old_raw_events(days).await {
                    Ok(n) => tracing::info!(
                        "Retention: deleted {} raw_events older than {} days",
                        n,
                        days
                    ),
                    Err(e) => tracing::warn!("Retention cleanup error: {:#}", e),
                }
            }
        });
    }

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENCY));

    tracing::info!("Event router ready, waiting for messages...");

    tokio::select! {
        _ = run_kafka_loop(
            kafka_consumer,
            Arc::clone(&semaphore),
            Arc::clone(&db),
            Arc::clone(&kafka_producer),
            Arc::clone(&correlator),
            Arc::clone(&identifiers),
            health_tx,
        ) => {}
        _ = casper_common::shutdown::shutdown_signal() => {
            tracing::info!("SIGTERM received, draining in-flight tasks...");
            let _ = semaphore.acquire_many(MAX_CONCURRENCY as u32).await;
            tracing::info!("All tasks drained, exiting.");
        }
    }

    Ok(())
}

async fn run_kafka_loop(
    kafka_consumer: KafkaConsumer,
    semaphore: Arc<Semaphore>,
    db: Arc<PostgresDB>,
    kafka_producer: Arc<KafkaProducer>,
    correlator: Arc<TransactionCorrelator>,
    identifiers: Arc<IdentifierRegistry>,
    health_tx: tokio::sync::watch::Sender<std::time::Instant>,
) {
    loop {
        let message = match kafka_consumer.poll(Duration::from_secs(5)).await {
            Ok(Some(msg)) => msg,
            Ok(None) => continue,
            Err(e) => {
                tracing::error!("Error polling Kafka: {}", e);
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        let _ = health_tx.send(std::time::Instant::now());
        metrics::counter!("casper_router_messages_processed_total").increment(1);

        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let db = Arc::clone(&db);
        let kafka_producer = Arc::clone(&kafka_producer);
        let correlator = Arc::clone(&correlator);
        let identifiers = Arc::clone(&identifiers);

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
    let raw_event = RawEvent::try_from(message)?;

    tracing::debug!(
        "Processing event: id={}, type={}",
        raw_event.id,
        raw_event.event_type
    );

    let enriched_tx = match correlator.correlate(&raw_event)? {
        Some(tx) => tx,
        None => {
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

    metrics::counter!("casper_router_correlations_completed_total").increment(1);

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

    kafka_producer
        .publish_json(
            ENRICHED_CHAIN_EVENTS,
            &enriched_tx.tx_hash,
            &enriched_tx,
        )
        .await
        .context("Failed to publish to enriched.chain_events")?;

    tracing::debug!(
        "Published to {}: {}",
        ENRICHED_CHAIN_EVENTS,
        enriched_tx.tx_hash
    );

    let app_events = identifiers
        .identify_all(&enriched_tx)
        .context("Failed to run app identifiers")?;

    for (topic, app_event) in app_events {
        kafka_producer
            .publish_json(&topic, &app_event.tx_hash, &app_event)
            .await
            .with_context(|| format!("Failed to publish to topic {}", topic))?;

        metrics::counter!("casper_router_app_events_total", "topic" => topic.clone())
            .increment(1);

        tracing::info!(
            "Published app event: topic={}, tx_hash={}",
            topic,
            app_event.tx_hash
        );
    }

    Ok(())
}
