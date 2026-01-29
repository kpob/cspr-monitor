use anyhow::Result;
use sqlx::{PgPool, Pool, Postgres};

pub const TRANSACTION_ACCEPTED: &str = "TransactionAccepted";
pub const TRANSACTION_PROCESSED: &str = "TransactionProcessed";
pub const BLOCK_ADDED: &str = "BlockAdded";

#[derive(Debug, PartialEq)]
pub struct Event {
    pub id: i64,
    pub payload: serde_json::Value,
}

#[async_trait::async_trait]
pub trait Database {
    async fn insert_event(&self, event_type: &str, event: &str) -> Result<()>;
    async fn get_events(&self, event_type: &str) -> Result<Vec<Event>>;
}

pub struct PostgresDB {
    executor: Pool<Postgres>,
}

impl PostgresDB {
    pub async fn new() -> Result<Self> {
        let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL not set");
        let executor = PgPool::connect(&db_url).await?;
        println!("Connected to PostgreSQL!");

        Ok(Self { executor })
    }
}

#[async_trait::async_trait]
impl Database for PostgresDB {
    async fn insert_event(&self, event_type: &str, event: &str) -> Result<()> {
        let json = serde_json::to_value(event)?;

        sqlx::query("INSERT INTO raw_events (event_type, payload) VALUES ($1, $2)")
            .bind(event_type)
            .bind(json)
            .execute(&self.executor)
            .await?;
        Ok(())
    }

    async fn get_events(&self, event_type: &str) -> Result<Vec<Event>> {
        let events = sqlx::query_as!(
            Event,
            "SELECT id, payload FROM raw_events WHERE event_type = $1 LIMIT 100",
            event_type
        )
        .fetch_all(&self.executor)
        .await?;

        Ok(events)
    }
}
