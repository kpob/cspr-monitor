---
name: new-downstream-consumer
description: >
  Scaffold a new downstream Kafka consumer crate in the casper-monitor workspace. Use when the
  user wants to create a new service, app, consumer, or crate that listens to Kafka topics
  (enriched.chain_events, apps.contracts, apps.exchanges, apps.native, etc.). Also trigger on:
  "new consumer", "new service", "add a crate", "scaffold app".
disable-model-invocation: true
---

# New Downstream Consumer Scaffold

Create a new Kafka consumer crate following the established workspace patterns.

## Step 1: Read existing patterns

Read these files to understand current conventions before generating any code:

- `casper-delta-filter/Cargo.toml` — simplest consumer Cargo.toml
- `casper-delta-filter/src/main.rs` — canonical minimal consumer pattern
- `casper-event-consumer/src/lib.rs` — EventConsumer builder + EventHandler trait API
- `Cargo.toml` — workspace members list and shared dependency versions
- `Dockerfile` — binary build targets and runtime COPY pattern
- `docker-compose.yml` — service entry pattern (dev stack)
- `docker-compose.prod.yml` — service entry pattern (prod stack)

## Step 2: Ask the user

Before scaffolding, clarify:

1. **Crate name** — e.g., `casper-price-tracker` (use `casper-` prefix for pipeline services)
2. **Kafka topic(s)** to consume — e.g., `apps.exchanges`, `enriched.chain_events`, `apps.native`
3. **Consumer group ID** — e.g., `price-tracker-v1`
4. **What the handler should do** — log, filter, aggregate, call external API, write to DB, etc.
5. **Does it need a web dashboard?** — If yes, also reference `casper-exchange-monitor` or `web-whale-activity` for the Axum + HTML pattern
6. **Does it need PostgreSQL?** — If yes, add sqlx dependency and DATABASE_URL handling

## Step 3: Create the crate

### 3a. Create directory and Cargo.toml

```toml
[package]
name = "<crate-name>"
version = "0.1.0"
edition.workspace = true

[dependencies]
casper-common = { path = "../casper-common" }
casper-event-consumer = { path = "../casper-event-consumer" }
tokio = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
dotenv = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
async-trait = { workspace = true }
```

### 3b. Create src/main.rs

Follow the `casper-delta-filter` pattern exactly:

```rust
use anyhow::Result;
use casper_event_consumer::{EnrichedEvent, EventConsumer, EventHandler};

struct MyHandler;

#[async_trait::async_trait]
impl EventHandler for MyHandler {
    async fn handle(&self, event: EnrichedEvent) -> Result<()> {
        // User's processing logic here
        tracing::info!("Received: tx_hash={}, status={}", event.tx_hash, event.lifecycle.status);
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    casper_common::init_tracing();

    tracing::info!("Starting <service-name>");

    let consumer = EventConsumer::builder()
        .brokers(std::env::var("KAFKA_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".to_string()))
        .topics(vec!["<topic>"])
        .group_id("<group-id>")
        .build()?;

    consumer.subscribe(MyHandler).await?;
    Ok(())
}
```

### 3c. Add to workspace

In root `Cargo.toml`, add the crate name to the `members` array.

### 3d. Add to Dockerfile

In the builder stage, add:
```dockerfile
RUN cargo build --release --bin <binary-name>
```

In the runtime stage, add:
```dockerfile
COPY --from=builder /app/target/release/<binary-name> /usr/local/bin/<binary-name>
```

### 3e. Add to docker-compose.yml (dev)

Add a service entry following the pattern of existing consumers (depends_on kafka + event-router).

### 3f. Add to docker-compose.prod.yml (prod)

Same pattern but without nctl dependency and using .env.prod variables.

## Step 4: Verify

1. `cargo check -p <crate-name>` — compiles without errors
2. `cargo build -p <crate-name>` — binary builds successfully
3. Add any new Kafka topic if needed: update the `kafka-init` service in docker-compose
