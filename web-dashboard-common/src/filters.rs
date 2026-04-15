use std::collections::HashMap;

use serde::Serialize;

use crate::state::{DashboardState, EventRecord};

#[derive(Serialize, Default)]
pub struct TargetSummary {
    pub tx_count: u64,
    pub total_amount: u64,
    pub actions: HashMap<String, u64>,
}

#[derive(Serialize)]
pub struct AddressEventsResponse {
    pub address: String,
    pub events: Vec<EventRecord>,
    pub targets: HashMap<String, TargetSummary>,
}

pub fn filter_by_address(state: &DashboardState, address: &str) -> AddressEventsResponse {
    let mut events = Vec::new();
    let mut targets: HashMap<String, TargetSummary> = HashMap::new();

    for record in state.events.iter() {
        let is_sender = record.actor_address == address;
        let is_target = record.target == address;
        if !is_sender && !is_target { continue; }
        events.push(record.clone());
        let other = if is_sender { record.target.clone() } else { record.actor_address.clone() };
        let entry = targets.entry(other).or_default();
        entry.tx_count += 1;
        entry.total_amount += record.amount;
        *entry.actions.entry(record.action.clone()).or_default() += 1;
    }

    AddressEventsResponse { address: address.to_string(), events, targets }
}

#[derive(Serialize)]
pub struct FilteredResponse {
    pub value: String,
    pub events: Vec<EventRecord>,
}

pub fn filter_by_field(state: &DashboardState, field: &str, value: &str) -> FilteredResponse {
    let events = state
        .events
        .iter()
        .filter(|r| match field {
            "actor" => r.actor == value,
            "actor_address" => r.actor_address == value,
            "action" => r.action == value,
            "target" => r.target == value,
            "tx_hash" => r.tx_hash == value,
            _ => false,
        })
        .cloned()
        .collect();
    FilteredResponse { value: value.to_string(), events }
}
