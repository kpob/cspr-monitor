use anyhow::Result;
use casper_common::{Database, PostgresDB, RawEvent, TRANSACTION_ACCEPTED, TRANSACTION_PROCESSED};
use casper_types::{Transaction, TransactionHash, execution::ExecutionResult};
use std::sync::Arc;
use tokio::{sync::Semaphore, time::sleep};

const MAX_CONCURRENCY: usize = 4;
const BATCH_SIZE: i64 = 100;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let db = PostgresDB::new().await?;
    process_events(&db).await?;

    Ok(())
}

async fn process_events(db: &PostgresDB) -> Result<()> {
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENCY));

    loop {
        let events = db.get_unprocessed_events(BATCH_SIZE).await?;
        println!("events: {:#?}", events.len());
        // No backlog → go idle
        if events.is_empty() {
            println!("No backlog → go idle");
            sleep(tokio::time::Duration::from_secs(10)).await;
            continue;
        }

        for event in events {
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let db = db.clone();

            tokio::spawn(async move {
                let _permit = permit;

                if let Err(e) = process_event(&db, &event).await {
                    println!("processing failed: {:?}", e);
                    return;
                }

                db.mark_processed(event.id).await.unwrap();
            });
        }
    }
}

async fn process_event(db: &PostgresDB, event: &RawEvent) -> Result<()> {
    match event.event_type.as_str() {
        TRANSACTION_ACCEPTED => {
            let transaction = serde_json::from_value::<Transaction>(
                event.payload["TransactionAccepted"].clone(),
            )?;
            let sender = transaction.initiator_addr().account_hash().to_hex_string();

            db.insert_accepted_tx(
                &transaction.hash().to_hex_string(),
                &event.received_at.to_string(),
                &sender,
                event.payload.clone(),
            )
            .await?;
        }
        TRANSACTION_PROCESSED => {
            let data = &event.payload["TransactionProcessed"];
            let transaction =
                serde_json::from_value::<ExecutionResult>(data["execution_result"].clone())?;
            let hash = serde_json::from_value::<TransactionHash>(data["transaction_hash"].clone())?;
            let timestamp = data["timestamp"].as_str().unwrap();
            let status = match transaction.error_message() {
                Some(_) => "failure",
                None => "success",
            };
            db.insert_processed_tx(
                &hash.to_hex_string(),
                timestamp,
                status,
                event.payload.clone(),
            )
            .await?;
        }
        _ => {
            // no-op
        }
    }
    Ok(())
}
