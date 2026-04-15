use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use casper_event_consumer::{EnrichedEvent, EventHandler};
use tokio::sync::{Mutex, broadcast};

use crate::state::{DashboardState, EventRecord};
use crate::EventMapper;

pub struct DashboardHandler<M: EventMapper> {
    pub mapper: Arc<M>,
    pub metric_name: Arc<str>,
    pub broadcast_tx: broadcast::Sender<EventRecord>,
    pub state: Arc<Mutex<DashboardState>>,
}

#[async_trait]
impl<M: EventMapper> EventHandler for DashboardHandler<M> {
    async fn handle(&self, event: EnrichedEvent) -> Result<()> {
        let Some(record) = self.mapper.map(&event) else { return Ok(()); };

        metrics::counter!(
            (*self.metric_name).to_string(),
            "actor" => record.actor.clone(),
            "action" => record.action.clone(),
        ).increment(1);

        tracing::info!(
            "[{}] {} {} motes → {} (tx={}, status={})",
            record.actor,
            record.action.to_uppercase(),
            record.amount,
            record.target,
            &record.tx_hash[..record.tx_hash.len().min(12)],
            record.status,
        );

        {
            let mut guard = self.state.lock().await;
            guard.push_event(record.clone());
        }
        let _ = self.broadcast_tx.send(record);
        Ok(())
    }
}
