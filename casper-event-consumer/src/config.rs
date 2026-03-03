use serde::{Deserialize, Serialize};

/// Configuration for Kafka consumer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KafkaConfig {
    pub brokers: String,
    pub topics: Vec<String>,
    pub group_id: String,
}

impl KafkaConfig {
    /// Create a new Kafka configuration
    pub fn new(brokers: impl Into<String>, topics: Vec<String>, group_id: impl Into<String>) -> Self {
        Self {
            brokers: brokers.into(),
            topics,
            group_id: group_id.into(),
        }
    }

    /// Load configuration from environment variables
    pub fn from_env() -> anyhow::Result<Self> {
        let brokers = std::env::var("KAFKA_BOOTSTRAP")
            .unwrap_or_else(|_| "localhost:9092".to_string());

        let topics_str = std::env::var("KAFKA_TOPICS")
            .map_err(|_| anyhow::anyhow!("KAFKA_TOPICS environment variable is required"))?;
        let topics: Vec<String> = topics_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        let group_id = std::env::var("KAFKA_GROUP_ID")
            .map_err(|_| anyhow::anyhow!("KAFKA_GROUP_ID environment variable is required"))?;

        Ok(Self {
            brokers,
            topics,
            group_id,
        })
    }
}
