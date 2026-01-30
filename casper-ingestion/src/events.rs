use anyhow::Result;
use casper_common::{BLOCK_ADDED, TRANSACTION_ACCEPTED, TRANSACTION_PROCESSED};
use eventsource_stream::{Event, Eventsource};
use futures::Stream;
use futures_util::TryStreamExt;

pub async fn event_stream() -> Result<impl Stream<Item = Result<Event, anyhow::Error>>> {
    let client = reqwest::Client::new();
    let sse_url = std::env::var("LIVENET_EVENT_ADDRESS").expect("LIVENET_EVENT_ADDRESS not set");
    let res = client.get(sse_url).send().await?;
    println!("Response: {:#?}", res);
    let es = res
        .bytes_stream()
        .eventsource()
        .map_err(anyhow::Error::from);
    Ok(es)
}

#[derive(Debug, PartialEq)]
pub enum EventImportance {
    Noise,
    Relevant(&'static str),
}

impl From<&Event> for EventImportance {
    fn from(event: &Event) -> Self {
        // strip the leading {"
        let data = &event.data[2..];
        if data.starts_with(TRANSACTION_ACCEPTED) {
            return EventImportance::Relevant(TRANSACTION_ACCEPTED);
        }
        if data.starts_with(TRANSACTION_PROCESSED) {
            return EventImportance::Relevant(TRANSACTION_PROCESSED);
        }
        if data.starts_with(BLOCK_ADDED) {
            return EventImportance::Relevant(BLOCK_ADDED);
        }
        EventImportance::Noise
    }
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
        assert!(EventImportance::from(&noise) == EventImportance::Noise);
    }

    #[test]
    fn accepts_processed_transactions() {
        let tx = Event {
            event: "message".into(),
            data: "{\"TransactionProcessed\":{}}".into(),
            id: "".into(),
            retry: None,
        };
        assert!(EventImportance::from(&tx) == EventImportance::Relevant(TRANSACTION_PROCESSED));
    }
}
