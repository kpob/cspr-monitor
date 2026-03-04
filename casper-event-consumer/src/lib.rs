use anyhow::{Context, Result};
use async_trait::async_trait;
use rdkafka::{
    config::ClientConfig,
    consumer::{Consumer, StreamConsumer},
    Message,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub mod config;

pub use config::KafkaConfig;

/// Trait for handling enriched events
///
/// Implement this trait to define your event processing logic
#[async_trait]
pub trait EventHandler: Send + Sync {
    async fn handle(&self, event: EnrichedEvent) -> Result<()>;
}

/// Enriched event with transaction lifecycle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedEvent {
    pub event_id: String,
    pub tx_hash: String,
    pub timestamp: String,
    pub lifecycle: TransactionLifecycle,
    pub app_data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionLifecycle {
    pub accepted_at: String,
    pub processed_at: String,
    pub status: String,
    pub sender: String,
}

/// Event consumer for subscribing to Kafka topics
pub struct EventConsumer {
    consumer: StreamConsumer,
    config: KafkaConfig,
}

impl EventConsumer {
    /// Create a new EventConsumerBuilder
    pub fn builder() -> EventConsumerBuilder {
        EventConsumerBuilder::new()
    }

    /// Subscribe to topics and process events with the provided handler
    ///
    /// This will run indefinitely, processing events as they arrive
    pub async fn subscribe<H: EventHandler>(&self, handler: H) -> Result<()> {
        tracing::info!(
            "Starting event consumer: group_id={}, topics={:?}",
            self.config.group_id,
            self.config.topics
        );

        loop {
            match self.consumer.recv().await {
                Ok(message) => {
                    // Extract payload
                    let payload = match message.payload() {
                        Some(p) => p,
                        None => {
                            tracing::warn!("Received message with no payload");
                            continue;
                        }
                    };

                    // Deserialize event
                    let event: EnrichedEvent = match serde_json::from_slice(payload) {
                        Ok(e) => e,
                        Err(e) => {
                            tracing::error!("Failed to deserialize event: {}", e);
                            continue;
                        }
                    };

                    tracing::debug!("Received event: tx_hash={}", event.tx_hash);

                    // Process event with handler
                    match handler.handle(event.clone()).await {
                        Ok(()) => {
                            // Commit offset after successful processing
                            if let Err(e) = self.consumer.commit_consumer_state(
                                rdkafka::consumer::CommitMode::Sync,
                            ) {
                                tracing::error!("Failed to commit offset: {}", e);
                            } else {
                                tracing::debug!("Successfully processed and committed: {}", event.tx_hash);
                            }
                        }
                        Err(e) => {
                            tracing::error!(
                                "Handler failed to process event {}: {}",
                                event.tx_hash,
                                e
                            );
                            // Don't commit offset on failure - will retry on restart
                            // For production, consider DLQ or retry logic
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Error receiving message: {}", e);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }
}

/// Builder for EventConsumer
pub struct EventConsumerBuilder {
    brokers: Option<String>,
    topics: Vec<String>,
    group_id: Option<String>,
}

impl EventConsumerBuilder {
    pub fn new() -> Self {
        Self {
            brokers: None,
            topics: Vec::new(),
            group_id: None,
        }
    }

    /// Set Kafka brokers (default: localhost:9092)
    pub fn brokers(mut self, brokers: impl Into<String>) -> Self {
        self.brokers = Some(brokers.into());
        self
    }

    /// Set topics to subscribe to
    pub fn topics(mut self, topics: Vec<impl Into<String>>) -> Self {
        self.topics = topics.into_iter().map(|t| t.into()).collect();
        self
    }

    /// Set consumer group ID (required)
    pub fn group_id(mut self, group_id: impl Into<String>) -> Self {
        self.group_id = Some(group_id.into());
        self
    }

    /// Build the EventConsumer
    pub fn build(self) -> Result<EventConsumer> {
        let group_id = self
            .group_id
            .ok_or_else(|| anyhow::anyhow!("group_id is required"))?;

        if self.topics.is_empty() {
            return Err(anyhow::anyhow!("At least one topic is required"));
        }

        let brokers = self
            .brokers
            .unwrap_or_else(|| "localhost:9092".to_string());

        let config = KafkaConfig {
            brokers: brokers.clone(),
            topics: self.topics.clone(),
            group_id: group_id.clone(),
        };

        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &brokers)
            .set("group.id", &group_id)
            .set("enable.auto.commit", "false") // Manual commit for exactly-once
            .set("auto.offset.reset", "earliest")
            .create()
            .context("Failed to create Kafka consumer")?;

        let topic_refs: Vec<&str> = self.topics.iter().map(|s| s.as_str()).collect();
        consumer
            .subscribe(&topic_refs)
            .context("Failed to subscribe to topics")?;

        Ok(EventConsumer { consumer, config })
    }
}

impl Default for EventConsumerBuilder {
    fn default() -> Self {
        Self::new()
    }
}
