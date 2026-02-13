//! Kafka topic name constants

/// Raw blockchain events from the ingestion service
pub const RAW_CHAIN_EVENTS: &str = "raw.chain_events";

/// Enriched transaction events after correlation
pub const ENRICHED_CHAIN_EVENTS: &str = "enriched.chain_events";

/// Contract interaction events
pub const APPS_CONTRACTS: &str = "apps.contracts";

/// Exchange/DEX interaction events
pub const APPS_EXCHANGES: &str = "apps.exchanges";

/// Arbitrage opportunity signals
pub const SIGNALS_ARBITRAGE: &str = "signals.arbitrage";
