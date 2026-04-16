# syntax=docker/dockerfile:1

FROM rust:bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev librdkafka-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY casper-common/ casper-common/
COPY casper-delta-filter/ casper-delta-filter/
COPY casper-event-consumer/ casper-event-consumer/
COPY casper-event-router/ casper-event-router/
COPY casper-exchange-monitor/ casper-exchange-monitor/
COPY casper-ingestion/ casper-ingestion/
COPY casper-simulator/ casper-simulator/
COPY web-dashboard-common/ web-dashboard-common/
COPY web-whale-activity/ web-whale-activity/

RUN cargo +nightly build --release \
    --bin casper-ingestion \
    --bin casper-event-router \
    --bin casper-exchange-monitor \
    --bin web-whale-activity \
    --bin simulator

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates libssl3 librdkafka1 curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# project_root crate searches upward for Cargo.lock to determine project root.
# Without it, find_wasm_file_path fails silently (no log output at all).
COPY --from=builder /app/Cargo.lock /app/Cargo.lock

# Static config files read at runtime by event-router (DEFAULT_CONFIG_PATH = "config/apps_config.json")
COPY casper-event-router/config/ config/

# Rename binaries to match docker-compose command names
COPY --from=builder /app/target/release/casper-ingestion /usr/local/bin/ingestion
COPY --from=builder /app/target/release/casper-event-router /usr/local/bin/event-router
COPY --from=builder /app/target/release/casper-exchange-monitor /usr/local/bin/exchange-monitor
COPY --from=builder /app/target/release/web-whale-activity /usr/local/bin/whale-activity
COPY --from=builder /app/target/release/simulator /usr/local/bin/simulator

# Framework static assets (design system + shared JS + widget JS)
COPY --from=builder /app/web-dashboard-common/static /app/static

# Framework templates are compiled into the binary by askama; no runtime copy needed.

# Per-dashboard overrides (custom widgets + dashboard-specific JS/CSS) — overlay after framework
COPY --from=builder /app/web-whale-activity/static /app/static
COPY --from=builder /app/casper-exchange-monitor/static /app/static

# Dashboard TOML configs — keep crate-relative paths so DASHBOARD_CONFIG=... can resolve them
COPY --from=builder /app/casper-exchange-monitor/dashboard.toml /app/casper-exchange-monitor/dashboard.toml
COPY --from=builder /app/web-whale-activity/dashboard.toml /app/web-whale-activity/dashboard.toml
