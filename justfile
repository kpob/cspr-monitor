BOOTSTRAP := "localhost:9092"

set dotenv-load := true

default:
  @just --list

# --- Docker compose ---
# Start all services in the background
docker-up:
  docker compose up -d

# Stop and remove containers
docker-down:
  docker compose down

# Build Docker image(s) without starting
docker-build:
  docker compose build

# Rebuild image(s) and restart services
docker-rebuild:
  docker compose up -d --build

# Full rebuild from scratch, ignoring all cached layers
docker-build-clean:
  docker compose build --no-cache

# Restart all services (full down → up cycle)
docker-restart:
  docker compose down
  docker compose up -d

# Follow logs for all services
docker-logs:
  docker compose logs -f

# Follow logs for a specific service: ingestion, event-router, kafka, postgres, nctl
docker-logs-ingestion:
  docker compose logs -f ingestion

docker-logs-router:
  docker compose logs -f event-router

docker-logs-kafka:
  docker compose logs -f kafka

docker-logs-deployer:
  docker compose logs contract-deployer

docker-logs-simulator:
  docker compose logs simulator

# Show running containers and their health status
docker-ps:
  docker compose ps

# --- Status / monitoring ---

# Overall pipeline status: services + consumer lag + DB row counts
docker-status:
  #!/usr/bin/env bash
  echo "=== Services ==="
  docker compose ps
  echo ""
  echo "=== Consumer Lag (event-router) ==="
  docker compose exec kafka kafka-consumer-groups \
    --bootstrap-server {{BOOTSTRAP}} \
    --describe \
    --group event-router 2>/dev/null || echo "  (kafka not ready)"
  echo ""
  echo "=== Database row counts ==="
  docker compose exec postgres psql -U dev -d ingest_dev -t -c \
    "SELECT format('  %-16s %s rows', relname, n_live_tup) \
     FROM pg_stat_user_tables ORDER BY relname;" \
    2>/dev/null || echo "  (postgres not ready)"

# Show consumer group lag for the event-router
docker-lag:
  docker compose exec kafka kafka-consumer-groups \
    --bootstrap-server {{BOOTSTRAP}} \
    --describe \
    --group event-router

# Exact row counts for key pipeline tables
docker-db-stats:
  docker compose exec postgres psql -U dev -d ingest_dev -c \
    "SELECT 'raw_events'   AS tbl, COUNT(*) AS rows FROM raw_events \
     UNION ALL \
     SELECT 'tx_lifecycle', COUNT(*) FROM tx_lifecycle;"

# --- Kafka CLI helpers ---

kafka-topics:
  docker compose exec kafka kafka-topics --bootstrap-server {{BOOTSTRAP}} --list

kafka-topic-create name partitions='6':
  docker compose exec kafka kafka-topics \
    --bootstrap-server {{BOOTSTRAP}} \
    --create \
    --topic {{name}} \
    --partitions {{partitions}} \
    --replication-factor 1

kafka-topic-delete name:
  docker compose exec kafka kafka-topics \
    --bootstrap-server {{BOOTSTRAP}} \
    --delete \
    --topic {{name}}

kafka-topic-describe name:
  docker compose exec kafka kafka-topics \
    --bootstrap-server {{BOOTSTRAP}} \
    --describe \
    --topic {{name}}

kafka-produce topic:
  docker compose exec -it kafka kafka-console-producer \
    --bootstrap-server {{BOOTSTRAP}} \
    --topic {{topic}}

kafka-consume topic:
  docker compose exec -it kafka kafka-console-consumer \
    --bootstrap-server {{BOOTSTRAP}} \
    --topic {{topic}} \
    --from-beginning

kafka-consume-latest topic:
  docker compose exec -it kafka kafka-console-consumer \
    --bootstrap-server {{BOOTSTRAP}} \
    --topic {{topic}}

kafka-groups:
  docker compose exec kafka kafka-consumer-groups \
    --bootstrap-server {{BOOTSTRAP}} \
    --list

kafka-group-describe group:
  docker compose exec kafka kafka-consumer-groups \
    --bootstrap-server {{BOOTSTRAP}} \
    --describe \
    --group {{group}}

# Create the standard pipeline topics
kafka-init-topics:
  just kafka-topic-create raw.chain_events 12
  just kafka-topic-create enriched.chain_events 12
  just kafka-topic-create signals.arbitrage 3

# --- Local development ---
# Run ingestion service locally (requires .env and local librdkafka)
run-ingestion:
    cargo run --bin casper-ingestion

# Run event-router service locally (requires .env and local librdkafka)
run-router:
    cargo run --bin casper-event-router

# Install system dependencies (librdkafka)
install-deps:
  #!/usr/bin/env bash
  set -euo pipefail
  if [[ "{{os()}}" == "macos" ]]; then
    echo "Installing librdkafka on macOS..."
    brew install librdkafka
  elif [[ "{{os()}}" == "linux" ]]; then
    echo "Installing librdkafka on Linux..."
    if command -v apt-get &> /dev/null; then
      sudo apt-get update && sudo apt-get install -y librdkafka-dev
    elif command -v dnf &> /dev/null; then
      sudo dnf install -y librdkafka-devel
    elif command -v yum &> /dev/null; then
      sudo yum install -y librdkafka-devel
    elif command -v pacman &> /dev/null; then
      sudo pacman -S --noconfirm librdkafka
    else
      echo "Error: Unsupported Linux package manager"
      exit 1
    fi
  else
    echo "Error: Unsupported operating system: {{os()}}"
    exit 1
  fi
  echo "librdkafka installed successfully"