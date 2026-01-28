use eventsource_stream::Event;
use sqlx::{Pool, Postgres};

use crate::Error;

#[async_trait::async_trait]
pub trait DB {
    async fn connect(&mut self) -> Result<(), Error>;
    async fn insert_event(&self, event_type: &str, event: &Event) -> Result<(), Error>;
}

#[derive(Default)]
pub struct PostgresDB {
    executor: Option<Pool<Postgres>>,
}

#[async_trait::async_trait]
impl DB for PostgresDB {
    async fn connect(&mut self) -> Result<(), Error> {
        let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL not set");
        let pool: Pool<Postgres> = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await?;
        self.executor = Some(pool);
        Ok(())
    }

    async fn insert_event(&self, event_type: &str, event: &Event) -> Result<(), Error> {
        let executor = self.executor.as_ref().unwrap();
        sqlx::query("INSERT INTO raw_events (event_type, payload) VALUES ($1, $2)")
            .bind(event_type)
            .bind(serde_json::to_value(event.data.as_str())?)
            .execute(executor)
            .await?;
        Ok(())
    }
}
