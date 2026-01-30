use anyhow::Result;
use casper_common::{Database, PostgresDB};
use futures::StreamExt;

use crate::events::EventImportance;

mod events;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let db = PostgresDB::new().await?;

    let mut es = events::event_stream().await?;
    while let Some(event) = es.next().await {
        let event = event?;
        match EventImportance::from(&event) {
            EventImportance::Noise => continue,
            EventImportance::Relevant(ty) => {
                db.insert_event(&event.id, ty, &event.data).await?;
            }
        }
    }

    Ok(())
}
