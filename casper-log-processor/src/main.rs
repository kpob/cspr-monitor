use anyhow::Result;
use casper_common::{Database, PostgresDB, TRANSACTION_ACCEPTED};

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let db = PostgresDB::new().await?;
    let events = db.get_events(TRANSACTION_ACCEPTED).await?;
    events.into_iter().for_each(|event| {
        // let event = serde_json::from_value::<TransactionAcceptedEvent>(event.payload).unwrap();
        println!("event: {:#?}", event.payload);
        event.payload.get("TransactionAccepted").unwrap();
        println!("event: {:#?}", event);
    });

    Ok(())
}

#[derive(serde::Deserialize, Debug)]
pub struct TransactionAcceptedEvent {
    #[serde(rename = "TransactionAccepted")]
    pub transaction_accepted: TransactionAccepted,
}

#[derive(serde::Deserialize, Debug)]
pub struct TransactionAccepted {
    #[serde(rename = "Version1")]
    pub version_1: TransactionAcceptedV1,
}

#[derive(serde::Deserialize, Debug)]
pub struct TransactionAcceptedV1 {
    hash: String,
    payload: TransactionAcceptedPayload,
}

#[derive(serde::Deserialize, Debug)]
pub struct TransactionAcceptedPayload {
    initiator_addr: String,
    timestamp: String,
    ttl: String,
    chain_name: String,
    pricing_mode: PricingMode,
    fields: Fields,
}

#[derive(serde::Deserialize, Debug)]
pub struct PricingMode {
    payment_limited: PaymentLimited,
}

#[derive(serde::Deserialize, Debug)]
pub struct PaymentLimited {
    payment_amount: u64,
    gas_price_tolerance: u64,
    standard_payment: bool,
}

#[derive(serde::Deserialize, Debug)]
pub struct Fields {
    args: Args,
}

#[derive(serde::Deserialize, Debug)]
pub struct Args {
    named: Vec<Vec<String>>,
}

#[derive(serde::Deserialize, Debug)]
pub struct Approvals {
    signer: String,
    signature: String,
}
