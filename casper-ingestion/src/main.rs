use anyhow::Result;
use casper_common::{Database, KafkaProducer, PostgresDB, RAW_CHAIN_EVENTS};
use futures::StreamExt;

use crate::events::EventImportance;

mod events;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    casper_common::init_tracing();

    tracing::info!("Starting casper-ingestion service");

    casper_common::metrics::install_prometheus_exporter(9100);
    let health_tx = casper_common::health::spawn_health_server(9090, "casper-ingestion");

    let db = PostgresDB::new().await?;
    let kafka = KafkaProducer::new().await?;

    tokio::select! {
        _ = run_event_loop(&db, &kafka, &health_tx) => {}
        _ = casper_common::shutdown::shutdown_signal() => {
            tracing::info!("Shutting down casper-ingestion");
        }
    }

    Ok(())
}

async fn run_event_loop(
    db: &PostgresDB,
    kafka: &KafkaProducer,
    health_tx: &tokio::sync::watch::Sender<std::time::Instant>,
) {
    loop {
        match events::event_stream().await {
            Err(e) => {
                tracing::error!("Failed to connect to SSE endpoint: {}", e);
                metrics::counter!("casper_ingestion_sse_reconnects_total").increment(1);
            }
            Ok(mut es) => {
                while let Some(event) = es.next().await {
                    let event = match event {
                        Ok(e) => e,
                        Err(e) => {
                            tracing::error!("SSE stream error: {}", e);
                            break;
                        }
                    };
                    match EventImportance::from(&event) {
                        EventImportance::Noise => continue,
                        EventImportance::Relevant(ty) => {
                            metrics::counter!("casper_ingestion_events_total", "event_type" => ty)
                                .increment(1);

                            if let Err(e) = db.insert_raw_event(&event.id, ty, &event.data).await {
                                tracing::error!("Failed to insert event to DB: {}", e);
                                continue;
                            }

                            let _ = health_tx.send(std::time::Instant::now());

                            let key = format!("{}-{}", ty, event.id);
                            if let Err(e) =
                                kafka.publish(RAW_CHAIN_EVENTS, &key, &event.data).await
                            {
                                tracing::error!("Failed to publish to Kafka: {}", e);
                                metrics::counter!("casper_ingestion_kafka_errors_total")
                                    .increment(1);
                            } else {
                                tracing::debug!(
                                    "Published event {} to Kafka topic '{}'",
                                    event.id,
                                    RAW_CHAIN_EVENTS
                                );
                            }
                        }
                    }
                }
                tracing::warn!("SSE stream ended, reconnecting in 5s...");
                metrics::counter!("casper_ingestion_sse_reconnects_total").increment(1);
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}
