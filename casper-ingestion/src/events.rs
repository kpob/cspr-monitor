use eventsource_stream::{Event, Eventsource};
use futures::Stream;
use futures_util::TryStreamExt;

use crate::Error;

const TRANSACTION_ACCEPTED: &str = "TransactionAccepted";
const TRANSACTION_PROCESSED: &str = "TransactionProcessed";
const BLOCK_ADDED: &str = "BlockAdded";

#[derive(Debug, PartialEq)]
pub enum EventType {
    Noise,
    Relevant(&'static str),
}

impl From<&Event> for EventType {
    fn from(event: &Event) -> Self {
        if event.data.starts_with("{\"TransactionAccepted\"") {
            return EventType::Relevant(TRANSACTION_ACCEPTED);
        }
        if event.data.starts_with("{\"TransactionProcessed\"") {
            return EventType::Relevant(TRANSACTION_PROCESSED);
        }
        if event.data.starts_with("{\"BlockAdded\"") {
            return EventType::Relevant(BLOCK_ADDED);
        }
        EventType::Noise
    }
}

pub async fn event_stream() -> Result<impl Stream<Item = Result<Event, Error>>, Error> {
    let client = reqwest::Client::new();
    let sse_url = std::env::var("LIVENET_EVENT_ADDRESS").expect("LIVENET_EVENT_ADDRESS not set");
    let res = client.get(sse_url).send().await?;
    println!("Response: {:#?}", res);
    let es = res.bytes_stream().eventsource().map_err(Error::from);
    Ok(es)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_out_noise() {
        let noise = Event {
            event: "message".into(),
            data: "{\"FinalitySignature\":{}}".into(),
            id: "".into(),
            retry: None,
        };
        assert!(EventType::from(&noise) == EventType::Noise);
    }

    #[test]
    fn accepts_processed_transactions() {
        let tx = Event {
            event: "message".into(),
            data: "{\"TransactionProcessed\":{}}".into(),
            id: "".into(),
            retry: None,
        };
        assert!(EventType::from(&tx) == EventType::Relevant(TRANSACTION_PROCESSED));
    }
}
