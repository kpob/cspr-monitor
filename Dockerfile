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
COPY casper-ingestion/ casper-ingestion/
COPY odra-contracts/ odra-contracts/

RUN cargo +nightly build --release \
    --bin casper-ingestion \
    --bin casper-event-router \
    --bin deploy \
    --bin simulator

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates libssl3 librdkafka1 curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# project_root crate searches upward for Cargo.lock to determine project root.
# Without it, find_wasm_file_path fails silently (no log output at all).
COPY --from=builder /app/Cargo.lock /app/Cargo.lock

# Rename binaries to match docker-compose command names
COPY --from=builder /app/target/release/casper-ingestion /usr/local/bin/ingestion
COPY --from=builder /app/target/release/casper-event-router /usr/local/bin/event-router
COPY --from=builder /app/target/release/deploy /usr/local/bin/contract-deployer
COPY --from=builder /app/target/release/simulator /usr/local/bin/simulator
