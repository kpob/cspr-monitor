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

    let db = PostgresDB::new().await?;
    let kafka = KafkaProducer::new().await?;

    loop {
        match events::event_stream().await {
            Err(e) => {
                tracing::error!("Failed to connect to SSE endpoint: {}", e);
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
                            // Write to PostgreSQL (primary storage)
                            if let Err(e) = db.insert_raw_event(&event.id, ty, &event.data).await {
                                tracing::error!("Failed to insert event to DB: {}", e);
                                continue;
                            }

                            // Publish to Kafka (event stream)
                            // Don't fail if Kafka is unavailable - fallback to DB-only mode
                            let key = format!("{}-{}", ty, event.id);
                            if let Err(e) = kafka.publish(RAW_CHAIN_EVENTS, &key, &event.data).await {
                                tracing::error!("Failed to publish to Kafka: {}", e);
                            } else {
                                tracing::debug!("Published event {} to Kafka topic '{}'", event.id, RAW_CHAIN_EVENTS);
                            }
                        }
                    }
                }
                tracing::warn!("SSE stream ended, reconnecting in 5s...");
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}
