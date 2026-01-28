use eventsource_stream::EventStreamError;
use futures::StreamExt;
use std::fmt::Debug;

use crate::{
    db::{DB, PostgresDB},
    events::EventType,
};

mod db;
mod events;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut db = PostgresDB::default();
    db.connect().await?;

    println!("Connected to PostgreSQL!");

    let mut es = events::event_stream().await?;
    while let Some(event) = es.next().await {
        let event = event?;

        match EventType::from(&event) {
            EventType::Noise => continue,
            EventType::Relevant(e) => {
                db.insert_event(e, &event).await?;
            }
        }

        println!("event: {:?}", event);
    }

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to connect to PostgreSQL")]
    PostgresError(#[from] sqlx::Error),
    #[error("Failed to serialize event data")]
    SerdeJsonError(#[from] serde_json::Error),
    #[error("Failed to fetch event data")]
    ConnectionError(#[from] reqwest::Error),
    #[error("Failed to parse event")]
    EventSourceError(#[from] EventStreamError<reqwest::Error>),
}
