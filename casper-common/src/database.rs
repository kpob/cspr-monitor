use anyhow::Result;
use sqlx::{PgPool, Pool, Postgres};

use crate::models::{EnrichedTransaction, RawEvent};

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

const INSERT_TRANSACTION_LIFECYCLE: &str = "INSERT INTO tx_lifecycle (
    tx_hash,
    accepted_at,
    sender,
    raw_accepted,
    processed_at,
    status,
    raw_processed
)
VALUES ($1, $2, $3, $4, $5, $6, $7)
ON CONFLICT (tx_hash)
DO UPDATE SET
    accepted_at = EXCLUDED.accepted_at,
    sender = EXCLUDED.sender,
    raw_accepted = EXCLUDED.raw_accepted,
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

const GET_ENRICHED_TRANSACTIONS: &str =
    "SELECT tx_hash, accepted_at, processed_at, status, sender, raw_accepted, raw_processed
FROM tx_lifecycle
WHERE accepted_at IS NOT NULL
AND processed_at IS NOT NULL
LIMIT $1";

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
    async fn insert_transaction_lifecycle(
        &self,
        tx_hash: &str,
        accepted_at: &str,
        sender: &str,
        raw_accepted: serde_json::Value,
        processed_at: &str,
        status: &str,
        raw_processed: serde_json::Value,
    ) -> Result<()>;
    async fn get_unprocessed_events(&self, limit: i64) -> Result<Vec<RawEvent>>;
    async fn mark_processed(&self, id: i64) -> Result<()>;
    async fn get_enriched_transactions(&self, limit: i64) -> Result<Vec<EnrichedTransaction>>;
    async fn delete_old_raw_events(&self, older_than_days: i64) -> Result<u64>;
}

#[derive(Clone)]
pub struct PostgresDB {
    pool: Pool<Postgres>,
}

impl PostgresDB {
    pub async fn new() -> Result<Self> {
        let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL not set");
        let pool = PgPool::connect(&db_url).await?;
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

    async fn insert_transaction_lifecycle(
        &self,
        tx_hash: &str,
        accepted_at: &str,
        sender: &str,
        raw_accepted: serde_json::Value,
        processed_at: &str,
        status: &str,
        raw_processed: serde_json::Value,
    ) -> Result<()> {
        let accepted_at = accepted_at.parse::<chrono::DateTime<chrono::Utc>>()?;
        let processed_at = processed_at.parse::<chrono::DateTime<chrono::Utc>>()?;
        sqlx::query(INSERT_TRANSACTION_LIFECYCLE)
            .bind(tx_hash)
            .bind(accepted_at)
            .bind(sender)
            .bind(raw_accepted)
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

    async fn get_enriched_transactions(&self, limit: i64) -> Result<Vec<EnrichedTransaction>> {
        let txs: Vec<EnrichedTransaction> = sqlx::query_as(GET_ENRICHED_TRANSACTIONS)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

        Ok(txs)
    }

    async fn delete_old_raw_events(&self, older_than_days: i64) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM raw_events WHERE received_at < NOW() - ($1 * INTERVAL '1 day')",
        )
        .bind(older_than_days)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}
