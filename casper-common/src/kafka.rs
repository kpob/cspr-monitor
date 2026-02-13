use anyhow::{Context, Result};
use rdkafka::{
    config::ClientConfig,
    consumer::{Consumer, StreamConsumer},
    producer::{FutureProducer, FutureRecord},
    Message,
};
use std::time::Duration;

/// Kafka producer for publishing events
#[derive(Clone)]
pub struct KafkaProducer {
    producer: FutureProducer,
}

impl KafkaProducer {
    /// Create a new Kafka producer from environment variables
    pub async fn new() -> Result<Self> {
        let bootstrap_servers = std::env::var("KAFKA_BOOTSTRAP")
            .unwrap_or_else(|_| "localhost:9092".to_string());

        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", &bootstrap_servers)
            .set("message.timeout.ms", "5000")
            .set("queue.buffering.max.ms", "0") // Send immediately for low latency
            .create()
            .context("Failed to create Kafka producer")?;

        tracing::info!("Kafka producer initialized: {}", bootstrap_servers);

        Ok(Self { producer })
    }

    /// Publish a message to a Kafka topic
    ///
    /// # Arguments
    /// * `topic` - The Kafka topic to publish to
    /// * `key` - The message key (used for partitioning)
    /// * `payload` - The message payload as a JSON string
    pub async fn publish(&self, topic: &str, key: &str, payload: &str) -> Result<()> {
        let record = FutureRecord::to(topic)
            .key(key)
            .payload(payload);

        self.producer
            .send(record, Duration::from_secs(0))
            .await
            .map_err(|(err, _)| err)
            .context("Failed to send message to Kafka")?;

        tracing::debug!("Published to Kafka topic '{}' with key '{}'", topic, key);
        Ok(())
    }

    /// Publish a serializable object to Kafka
    pub async fn publish_json<T: serde::Serialize>(
        &self,
        topic: &str,
        key: &str,
        value: &T,
    ) -> Result<()> {
        let payload = serde_json::to_string(value)
            .context("Failed to serialize value to JSON")?;
        self.publish(topic, key, &payload).await
    }
}

/// Kafka consumer for consuming events
pub struct KafkaConsumer {
    consumer: StreamConsumer,
    #[allow(dead_code)]
    group_id: String,
}

impl KafkaConsumer {
    /// Create a new Kafka consumer
    ///
    /// # Arguments
    /// * `group_id` - The consumer group ID
    /// * `topics` - List of topics to subscribe to
    pub async fn new(group_id: &str, topics: Vec<&str>) -> Result<Self> {
        let bootstrap_servers = std::env::var("KAFKA_BOOTSTRAP")
            .unwrap_or_else(|_| "localhost:9092".to_string());

        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &bootstrap_servers)
            .set("group.id", group_id)
            .set("enable.auto.commit", "false") // Manual offset commit for exactly-once
            .set("auto.offset.reset", "earliest")
            .create()
            .context("Failed to create Kafka consumer")?;

        consumer
            .subscribe(&topics)
            .context("Failed to subscribe to topics")?;

        tracing::info!(
            "Kafka consumer initialized: group_id={}, topics={:?}",
            group_id,
            topics
        );

        Ok(Self {
            consumer,
            group_id: group_id.to_string(),
        })
    }

    /// Get a reference to the underlying consumer
    pub fn inner(&self) -> &StreamConsumer {
        &self.consumer
    }

    /// Commit the current offset
    pub fn commit(&self) -> Result<()> {
        self.consumer
            .commit_consumer_state(rdkafka::consumer::CommitMode::Sync)
            .context("Failed to commit offset")?;
        Ok(())
    }

    /// Poll for a single message with timeout
    pub async fn poll(&self, _timeout: Duration) -> Result<Option<KafkaMessage>> {
        match self.consumer.recv().await {
            Ok(message) => {
                let payload = message
                    .payload()
                    .ok_or_else(|| anyhow::anyhow!("Message has no payload"))?;

                let key = message
                    .key()
                    .map(|k| String::from_utf8_lossy(k).to_string())
                    .unwrap_or_default();

                let payload_str = String::from_utf8_lossy(payload).to_string();

                Ok(Some(KafkaMessage {
                    key,
                    payload: payload_str,
                    topic: message.topic().to_string(),
                    partition: message.partition(),
                    offset: message.offset(),
                }))
            }
            Err(e) => Err(anyhow::anyhow!("Failed to receive message: {}", e)),
        }
    }
}

/// A message received from Kafka
#[derive(Debug, Clone)]
pub struct KafkaMessage {
    pub key: String,
    pub payload: String,
    pub topic: String,
    pub partition: i32,
    pub offset: i64,
}

impl KafkaMessage {
    /// Deserialize the payload as JSON
    pub fn deserialize<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_str(&self.payload)
            .context("Failed to deserialize Kafka message payload")
    }
}
