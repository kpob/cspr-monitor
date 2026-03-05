//! Kafka topic name constants

/// Raw blockchain events from the ingestion service
pub const RAW_CHAIN_EVENTS: &str = "raw.chain_events";

/// Enriched transaction events after correlation
pub const ENRICHED_CHAIN_EVENTS: &str = "enriched.chain_events";

/// Contract interaction events for unclassified contracts (no matching app definition)
pub const APPS_UNCLASSIFIED: &str = "apps.unclassified";

/// Exchange/DEX interaction events
pub const APPS_EXCHANGES: &str = "apps.exchanges";

/// Arbitrage opportunity signals
pub const SIGNALS_ARBITRAGE: &str = "signals.arbitrage";

/// Native blockchain transactions (Transfer, Delegate, Undelegate, Redelegate, AddBid, WithdrawBid, ActivateBid, Session/WASM)
pub const APPS_NATIVE: &str = "apps.native";
