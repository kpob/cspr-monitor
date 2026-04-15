use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, broadcast};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub timestamp: String,
    pub actor: String,
    pub actor_address: String,
    pub action: String,
    pub amount: u64,
    pub target: String,
    pub tx_hash: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActorStats {
    pub actions: HashMap<String, u64>,
    pub tx_count: u64,
    pub total_amount: u64,
}

#[derive(Serialize)]
pub struct StatsResponse {
    pub actors: HashMap<String, ActorStats>,
    pub recent_events: Vec<EventRecord>,
}

pub struct DashboardState {
    pub events: VecDeque<EventRecord>,
    pub stats: HashMap<String, ActorStats>,
    pub max_events: usize,
}

impl DashboardState {
    pub fn new(max_events: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(max_events),
            stats: HashMap::new(),
            max_events,
        }
    }

    pub fn push_event(&mut self, record: EventRecord) {
        let entry = self.stats.entry(record.actor.clone()).or_default();
        entry.tx_count += 1;
        entry.total_amount += record.amount;
        *entry.actions.entry(record.action.clone()).or_default() += 1;
        self.events.push_front(record);
        while self.events.len() > self.max_events {
            self.events.pop_back();
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub broadcast_tx: broadcast::Sender<EventRecord>,
    pub state: Arc<Mutex<DashboardState>>,
    pub service_name: Arc<str>,
    pub config: Arc<crate::config::DashboardConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(actor: &str, action: &str, amount: u64) -> EventRecord {
        EventRecord {
            timestamp: "t".into(),
            actor: actor.into(),
            actor_address: "a".into(),
            action: action.into(),
            amount,
            target: "b".into(),
            tx_hash: "h".into(),
            status: "success".into(),
        }
    }

    #[test]
    fn push_event_updates_stats_and_total() {
        let mut s = DashboardState::new(3);
        s.push_event(rec("binance", "inflow", 100));
        s.push_event(rec("binance", "outflow", 40));
        let b = s.stats.get("binance").unwrap();
        assert_eq!(b.tx_count, 2);
        assert_eq!(b.total_amount, 140);
        assert_eq!(b.actions["inflow"], 1);
        assert_eq!(b.actions["outflow"], 1);
    }

    #[test]
    fn push_event_trims_to_max_events() {
        let mut s = DashboardState::new(2);
        s.push_event(rec("a", "x", 1));
        s.push_event(rec("b", "x", 2));
        s.push_event(rec("c", "x", 3));
        assert_eq!(s.events.len(), 2);
        assert_eq!(s.events.front().unwrap().actor, "c");
    }
}
