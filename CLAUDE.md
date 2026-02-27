# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run Commands

```bash
# System dependency (required before building)
just install-deps          # installs librdkafka via brew/apt/etc.

# Build all crates
cargo build

# Run individual services
just run-ingestion          # casper-ingestion
just run-router             # casper-event-router
cargo run --bin casper-log-processor
cargo run --bin casper-delta-filter

# Tests
cargo test                  # all unit tests
just ingest-test            # creates test DB schema + runs cargo test

# Infrastructure (Kafka + Kafka UI via Docker)
just up                     # start containers
just down                   # stop containers
just logs                   # follow logs

# Database setup
just init-db                # applies db/create.sql to ingest_dev

# Kafka topic management
just init-topics            # create raw.chain_events (12p), enriched.chain_events (12p), signals.arbitrage (3p)
just topics                 # list topics
just consume topic=raw.chain_events   # tail a topic from the beginning
just consume-latest topic=enriched.chain_events
```

## Environment Variables

Loaded from `.env` (via `dotenv`). Key variables:

| Variable | Used by | Notes |
|---|---|---|
| `LIVENET_EVENT_ADDRESS` | casper-ingestion | Casper node SSE endpoint |
| `DATABASE_URL` | all services with DB | PostgreSQL connection string |
| `KAFKA_BOOTSTRAP` | all services with Kafka | defaults to `localhost:9092` |
| `CONTRACT_CONFIG_PATH` | casper-event-router | defaults to `resources/known_contracts.json` |
| `EXCHANGE_CONFIG_PATH` | casper-event-router | defaults to `resources/exchanges.json` |

## Architecture Overview

This is a **Casper blockchain event monitoring pipeline** implemented as a Rust Cargo workspace. Events flow through four stages:

```
Casper Node (SSE)
      │
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
```

### Crates

- **`casper-common`** — shared library: PostgreSQL `Database` trait + `PostgresDB` impl, `KafkaProducer`/`KafkaConsumer` wrappers, shared model types (`RawEvent`, `EnrichedTransaction`, `AppEvent`, `TransactionLifecycle`), and Kafka topic name constants.

- **`casper-ingestion`** — connects to the Casper node SSE stream, filters for `TransactionAccepted`, `TransactionProcessed`, and `BlockAdded` events (discards everything else as "Noise"), dual-writes to PostgreSQL and publishes to `raw.chain_events` Kafka topic. Kafka failure is non-fatal (falls back to DB-only).

- **`casper-event-router`** — Kafka consumer on `raw.chain_events`. Correlates pairs of `TransactionAccepted` + `TransactionProcessed` events in-memory using `DashMap` (keyed by `tx_hash`) with a 5-minute timeout and background cleanup. Once correlated, writes to `tx_lifecycle` table and publishes `EnrichedTransaction` to `enriched.chain_events`. Then runs the `IdentifierRegistry` which dispatches to `apps.contracts` or `apps.exchanges` topics for matched transactions.

- **`casper-event-consumer`** — reusable library for downstream applications. Provides `EventConsumer` (builder pattern) and the `EventHandler` async trait. Consumers implement `EventHandler::handle(EnrichedEvent)` and call `consumer.subscribe(handler)` to process events in a loop with manual offset commit.

- **`casper-delta-filter`** — example downstream app using `casper-event-consumer`. Filters `apps.contracts` for the specific Casper Delta Market contract hash.

- **`casper-log-processor`** — legacy DB-polling processor. Reads unprocessed events from PostgreSQL, parses them using `casper-types`, and populates `tx_lifecycle`. Predates the Kafka-based router; runs in 100-event batches with 10-second idle sleep.

### App Identifiers (casper-event-router)

The `IdentifierRegistry` holds `Box<dyn AppIdentifier>` objects. Each identifier:
1. Inspects an `EnrichedTransaction`
2. Returns `Option<AppEvent>` (None = no match)
3. Has a `topic()` that determines where the `AppEvent` is published

Current identifiers:
- `ContractPatternIdentifier` — matches transactions targeting contract hashes listed in `resources/known_contracts.json` (format: `{"contracts": {"name": "hash"}}`)
- `ExchangeWalletIdentifier` — matches transactions from sender addresses in `resources/exchanges.json` (format: `{"exchanges": {"address": "name"}}`)

To add a new identifier, implement `AppIdentifier` in `casper-event-router/src/identifiers/` and register it in `IdentifierRegistry::new()`.

### Kafka Message Key Format

`casper-ingestion` publishes raw events with key `"EventType-EventID"` (e.g., `"TransactionAccepted-12345"`). The event router's `RawEvent::try_from(KafkaMessage)` parses this format — keep it consistent if modifying the ingestion service.

### Database Schema

Two main tables in PostgreSQL:
- **`raw_events`** (`id`, `event_type`, `payload` jsonb, `received_at`, `processed` bool) — written by `casper-ingestion`, read by `casper-log-processor`
- **`tx_lifecycle`** (`tx_hash` PK, `accepted_at`, `sender`, `raw_accepted` jsonb, `processed_at`, `status`, `raw_processed` jsonb) — written by both `casper-event-router` and `casper-log-processor` using `ON CONFLICT (tx_hash) DO UPDATE`

Schema is in `db/create.sql` (applied via `just init-db`).
