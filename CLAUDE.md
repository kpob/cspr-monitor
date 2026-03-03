# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run Commands

```bash
# System dependency (required before building)
just install-deps          # installs librdkafka via brew/apt/etc.

# Build all crates
cargo build

# Run individual services locally (requires .env and local librdkafka)
just run-ingestion          # casper-ingestion
just run-router             # casper-event-router

# Tests
cargo test                  # all unit tests

# Infrastructure (Docker — runs nctl, Kafka, PostgreSQL, ingestion, event-router)
just docker-up              # start all containers
just docker-down            # stop containers
just docker-logs            # follow all logs
just docker-rebuild         # rebuild image(s) and restart services
just docker-restart         # full down → up cycle
just docker-ps              # show running containers and health status
just docker-status          # pipeline status: services + consumer lag + DB row counts

# Service-specific logs
just docker-logs-ingestion
just docker-logs-router
just docker-logs-kafka
just docker-logs-deployer
just docker-logs-simulator

# Kafka topic management (via Docker)
just kafka-init-topics      # create raw.chain_events (12p), enriched.chain_events (12p), signals.arbitrage (3p)
just kafka-topics           # list topics
just kafka-consume topic=raw.chain_events       # tail a topic from the beginning
just kafka-consume-latest topic=enriched.chain_events
just kafka-topic-create name=<topic> partitions=6
just kafka-topic-delete name=<topic>
just kafka-groups           # list consumer groups
just kafka-group-describe group=event-router

# Monitoring
just docker-lag             # consumer group lag for event-router
just docker-db-stats        # row counts for raw_events and tx_lifecycle
```

## Environment Variables

Loaded from `.env` (via `dotenv`). Key variables:

| Variable | Used by | Notes |
|---|---|---|
| `LIVENET_EVENT_ADDRESS` | casper-ingestion | Casper node SSE endpoint |
| `DATABASE_URL` | all services with DB | PostgreSQL connection string |
| `KAFKA_BROKERS` | all services with Kafka | defaults to `localhost:9092` |
| `CONTRACT_CONFIG_PATH` | casper-event-router | defaults to `config/known_contracts.json` |
| `EXCHANGE_CONFIG_JSON_PATH` | casper-event-router | defaults to `config/exchanges.json` |
| `DEPLOYED_CONTRACTS_JSON_PATH` | casper-simulator, event-router | path to JSON written by simulator on startup and read by router |

## Architecture Overview

This is a **Casper blockchain event monitoring pipeline** implemented as a Rust Cargo workspace. The full Docker stack spins up a local Casper network (`nctl`), deploys test contracts, and runs the monitoring pipeline end-to-end.

```
nctl (local Casper network)
      │
      ▼
casper-simulator          → deploys WASM contracts, writes DEPLOYED_CONTRACTS_JSON_PATH
      │                   → runs infinite loop of test transactions (restarts = fresh redeploy)
      ▼
casper-ingestion          → PostgreSQL raw_events table
      │                   → Kafka: raw.chain_events
      ▼
casper-event-router       → correlates TransactionAccepted + TransactionProcessed by tx_hash
      │                   → PostgreSQL tx_lifecycle table
      │                   → Kafka: enriched.chain_events
      │                   → Kafka: apps.contracts  (if contract match)
      │                   → Kafka: apps.exchanges  (if exchange match)
      ▼
casper-delta-filter       (example downstream consumer)
  (or any custom app using casper-event-consumer library)

casper-simulator          → (see above)
```

### Crates

- **`casper-common`** — shared library: PostgreSQL `Database` trait + `PostgresDB` impl, `KafkaProducer`/`KafkaConsumer` wrappers, shared model types (`RawEvent`, `EnrichedTransaction`, `AppEvent`, `TransactionLifecycle`), and Kafka topic name constants.

- **`casper-ingestion`** — connects to the Casper node SSE stream, filters for `TransactionAccepted`, `TransactionProcessed`, and `BlockAdded` events (discards everything else as "Noise"), dual-writes to PostgreSQL and publishes to `raw.chain_events` Kafka topic. Kafka failure is non-fatal (falls back to DB-only).

- **`casper-event-router`** — Kafka consumer on `raw.chain_events`. Correlates pairs of `TransactionAccepted` + `TransactionProcessed` events in-memory using `DashMap` (keyed by `tx_hash`) with a 5-minute timeout and background cleanup. Once correlated, writes to `tx_lifecycle` table and publishes `EnrichedTransaction` to `enriched.chain_events`. Then runs the `IdentifierRegistry` which dispatches to `apps.contracts` or `apps.exchanges` topics for matched transactions.

- **`casper-event-consumer`** — reusable library for downstream applications. Provides `EventConsumer` (builder pattern) and the `EventHandler` async trait. Consumers implement `EventHandler::handle(EnrichedEvent)` and call `consumer.subscribe(handler)` to process events in a loop with manual offset commit.

- **`casper-delta-filter`** — example downstream app using `casper-event-consumer`. Filters `apps.contracts` for the specific Casper Delta Market contract hash.

- **`casper-simulator`** — single binary for the local dev environment: deploys ERC-20 and Ownable WASM contracts to nctl, writes contract addresses to `DEPLOYED_CONTRACTS_JSON_PATH` as JSON for the event-router to load, then runs an infinite loop of test transactions using multiple keypairs. On crash, Docker restarts it which triggers a fresh redeploy.

### App Identifiers (casper-event-router)

The `IdentifierRegistry` holds `Box<dyn AppIdentifier>` objects. Each identifier:
1. Inspects an `EnrichedTransaction`
2. Returns `Option<AppEvent>` (None = no match)
3. Has a `topic()` that determines where the `AppEvent` is published

Current identifiers:
- `ContractPatternIdentifier` — matches transactions targeting contract hashes. Loaded from `config/known_contracts.json` (or `CONTRACT_CONFIG_PATH`), supplemented by `DEPLOYED_CONTRACTS_JSON_PATH` at startup and by the `CONTRACT_PATTERNS` env var. Format: `{"contracts": {"Name": "hash"}}`
- `ExchangeWalletIdentifier` — matches transactions from sender addresses. Loaded from `config/exchanges.json` (or `EXCHANGE_CONFIG_JSON_PATH`), supplemented by the `EXCHANGE_ADDRESSES` env var (`address=Name,...`). Format: `{"exchanges": {"address": "name"}}`

Config files live in `casper-event-router/config/`. To add a new identifier, implement `AppIdentifier` in `casper-event-router/src/identifiers/` and register it in `IdentifierRegistry::new()`.

### Kafka Message Key Format

`casper-ingestion` publishes raw events with key `"EventType-EventID"` (e.g., `"TransactionAccepted-12345"`). The event router's `RawEvent::try_from(KafkaMessage)` parses this format — keep it consistent if modifying the ingestion service.

### Database Schema

Two main tables in PostgreSQL:
- **`raw_events`** (`id`, `event_type`, `payload` jsonb, `received_at`, `processed` bool) — written by `casper-ingestion`
- **`tx_lifecycle`** (`tx_hash` PK, `accepted_at`, `sender`, `raw_accepted` jsonb, `processed_at`, `status`, `raw_processed` jsonb) — written by `casper-event-router` using `ON CONFLICT (tx_hash) DO UPDATE`

Schema is in `migrations/init.sql` (auto-applied by PostgreSQL Docker container on first start).
