use anyhow::Result;
use casper_common::{Database, PostgresDB};
use futures::StreamExt;

use crate::events::EventImportance;

mod events;

#[tokio::main]
async fn main() -> Result<()> {
    let db = PostgresDB::new().await?;

    let mut es = events::event_stream().await?;
    while let Some(event) = es.next().await {
        let event = event?;

        match EventImportance::from(&event) {
            EventImportance::Noise => continue,
            EventImportance::Relevant(e) => {
                db.insert_event(e, &event.data).await?;
            }
        }

        println!("event: {:?}", event);
    }

    Ok(())
}
