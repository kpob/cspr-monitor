use serde_json::json;
use sqlx::{Pool, Postgres};

#[tokio::test]
async fn inserts_event_into_db() -> Result<(), Box<dyn std::error::Error>> {
    let db_url = std::env::var("DATABASE_URL")?;
    let pool: Pool<Postgres> = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await?;

    // Clean table before test
    sqlx::query("TRUNCATE raw_events").execute(&pool).await?;

    // Example event
    let event_type = "TransactionProcessed";
    let payload = json!({"tx": "0xabc", "height": 123});

    // Insert
    sqlx::query("INSERT INTO raw_events (event_type, payload) VALUES ($1, $2)")
        .bind(event_type)
        .bind(payload)
        .execute(&pool)
        .await?;

    // Assert it was inserted
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM raw_events WHERE event_type = $1")
        .bind(event_type)
        .fetch_one(&pool)
        .await?;

    assert_eq!(row.0, 1);

    Ok(())
}
