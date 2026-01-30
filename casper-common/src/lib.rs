use anyhow::Result;
use sqlx::{PgPool, Pool, Postgres, types::chrono};

pub const TRANSACTION_ACCEPTED: &str = "TransactionAccepted";
pub const TRANSACTION_PROCESSED: &str = "TransactionProcessed";
pub const BLOCK_ADDED: &str = "BlockAdded";

const INSERT_ACCEPTED_TX: &str = "INSERT INTO tx_lifecycle (
    tx_hash,
    accepted_at,
    sender,
    raw_accepted
)
VALUES ($1, $2, $3, $4)
ON CONFLICT (tx_hash)
DO UPDATE SET
    accepted_at = EXCLUDED.accepted_at,
    sender = EXCLUDED.sender,
    raw_accepted = EXCLUDED.raw_accepted";

const INSERT_PROCESSED_TX: &str = "INSERT INTO tx_lifecycle (
    tx_hash,
    processed_at,
    status,
    raw_processed
)
VALUES ($1, $2, $3, $4)
ON CONFLICT (tx_hash)
DO UPDATE SET
    processed_at = EXCLUDED.processed_at,
    status = EXCLUDED.status,
    raw_processed = EXCLUDED.raw_processed";

const MARK_PROCESSED: &str = "UPDATE raw_events SET processed = TRUE WHERE id = $1";

const GET_UNPROCESSED_EVENTS: &str = "SELECT id, received_at, event_type, payload
FROM raw_events
WHERE processed = FALSE
AND event_type != 'BlockAdded'
ORDER BY id
LIMIT $1";

#[derive(sqlx::FromRow, Debug, PartialEq, Eq)]
pub struct RawEvent {
    pub id: i64,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub received_at: chrono::DateTime<chrono::Utc>,
}

#[async_trait::async_trait]
pub trait Database {
    async fn insert_raw_event(&self, id: &str, event_type: &str, event: &str) -> Result<()>;
    async fn get_raw_events(&self, event_type: &str) -> Result<Vec<RawEvent>>;
    async fn insert_processed_tx(
        &self,
        tx_hash: &str,
        processed_at: &str,
        status: &str,
        raw_processed: serde_json::Value,
    ) -> Result<()>;
    async fn insert_accepted_tx(
        &self,
        tx_hash: &str,
        accepted_at: &str,
        sender: &str,
        raw_accepted: serde_json::Value,
    ) -> Result<()>;
    async fn get_unprocessed_events(&self, limit: i64) -> Result<Vec<RawEvent>>;
    async fn mark_processed(&self, id: i64) -> Result<()>;
}

#[derive(Clone)]
pub struct PostgresDB {
    pool: Pool<Postgres>,
}

impl PostgresDB {
    pub async fn new() -> Result<Self> {
        let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL not set");
        let pool = PgPool::connect(&db_url).await?;
        println!("Connected to PostgreSQL!");

        Ok(Self { pool })
    }
}

#[async_trait::async_trait]
impl Database for PostgresDB {
    async fn insert_raw_event(&self, id: &str, event_type: &str, event: &str) -> Result<()> {
        let json = serde_json::from_str::<serde_json::Value>(event)?;
        let id: i64 = id.parse()?;
        sqlx::query("INSERT INTO raw_events (id, event_type, payload) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(event_type)
            .bind(json)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get_raw_events(&self, event_type: &str) -> Result<Vec<RawEvent>> {
        let events: Vec<RawEvent> = sqlx::query_as(
            "SELECT id, event_type, payload, received_at FROM raw_events WHERE event_type = $1 LIMIT 100",
        )
        .bind(event_type)
        .fetch_all(&self.pool)
        .await?;

        Ok(events)
    }

    async fn insert_accepted_tx(
        &self,
        tx_hash: &str,
        accepted_at: &str,
        sender: &str,
        raw_accepted: serde_json::Value,
    ) -> Result<()> {
        let accepted_at = accepted_at.parse::<chrono::DateTime<chrono::Utc>>()?;
        sqlx::query(INSERT_ACCEPTED_TX)
            .bind(tx_hash)
            .bind(accepted_at)
            .bind(sender)
            .bind(raw_accepted)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn insert_processed_tx(
        &self,
        tx_hash: &str,
        processed_at: &str,
        status: &str,
        raw_processed: serde_json::Value,
    ) -> Result<()> {
        let processed_at = processed_at.parse::<chrono::DateTime<chrono::Utc>>()?;
        sqlx::query(INSERT_PROCESSED_TX)
            .bind(tx_hash)
            .bind(processed_at)
            .bind(status)
            .bind(raw_processed)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get_unprocessed_events(&self, limit: i64) -> Result<Vec<RawEvent>> {
        let events: Vec<RawEvent> = sqlx::query_as(GET_UNPROCESSED_EVENTS)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

        Ok(events)
    }

    async fn mark_processed(&self, id: i64) -> Result<()> {
        sqlx::query(MARK_PROCESSED)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
