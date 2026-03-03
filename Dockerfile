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

RUN cargo +nightly build --release \
    --bin casper-ingestion \
    --bin casper-event-router \
    --bin casper-exchange-monitor \
    --bin simulator

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates libssl3 librdkafka1 curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# project_root crate searches upward for Cargo.lock to determine project root.
# Without it, find_wasm_file_path fails silently (no log output at all).
COPY --from=builder /app/Cargo.lock /app/Cargo.lock

# Static config files read at runtime by event-router
COPY casper-event-router/config/ casper-event-router/config/

# Rename binaries to match docker-compose command names
COPY --from=builder /app/target/release/casper-ingestion /usr/local/bin/ingestion
COPY --from=builder /app/target/release/casper-event-router /usr/local/bin/event-router
COPY --from=builder /app/target/release/casper-exchange-monitor /usr/local/bin/exchange-monitor
COPY --from=builder /app/target/release/simulator /usr/local/bin/simulator
