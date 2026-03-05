BOOTSTRAP := "localhost:9092"


default:
  @just --list

# --- Docker compose (dev) ---
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

# Nuclear option: stop, remove, and rebuild everything
docker-rebuild-clean:
  docker compose down --volumes --remove-orphans
  docker compose build --no-cache
  docker compose up -d

# Restart all services (full down → up cycle)
docker-restart:
  docker compose down
  docker compose up -d

# Show running containers and their health status
docker-ps:
  docker compose ps

# Follow logs for all services
docker-logs:
  docker compose logs -f

# Follow logs for the ingestion service
docker-logs-ingestion:
  docker compose logs -f ingestion

# Follow logs for the event-router service
docker-logs-router:
  docker compose logs -f event-router

# Follow logs for the Kafka broker
docker-logs-kafka:
  docker compose logs -f kafka

# Follow logs for the deploy service
docker-logs-deployer:
  docker compose logs contract-deployer

# Follow logs for the simulator service
docker-logs-simulator:
  docker compose logs simulator

# --- Docker compose (prod) ---
# Start production services (no nctl, no simulator)
prod-up:
  docker compose --env-file .env.prod -f docker-compose.prod.yml up -d

# Stop production services
prod-down:
  docker compose --env-file .env.prod -f docker-compose.prod.yml down

# Rebuild and restart production services
prod-rebuild:
  docker compose --env-file .env.prod -f docker-compose.prod.yml up -d --build

# Follow logs for all production services
prod-logs:
  docker compose --env-file .env.prod -f docker-compose.prod.yml logs -f

# Show running production containers and health status
prod-ps:
  docker compose --env-file .env.prod -f docker-compose.prod.yml ps

# WARNING: destroys all prod data — wipe volumes and restart fresh
prod-reset:
  docker compose --env-file .env.prod -f docker-compose.prod.yml down -v
  docker compose --env-file .env.prod -f docker-compose.prod.yml up -d

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
  just kafka-topic-create apps.native 6

# --- Local development ---
# Run ingestion service locally (requires .env and local librdkafka)
run-ingestion:
    cargo run --bin casper-ingestion

# Run event-router service locally (requires .env and local librdkafka)
run-router:
    cargo run --bin casper-event-router

# Run simple consumer example locally (requires .env and local librdkafka)
run-simple-consumer:
    cargo run --example simple_consumer -p casper-event-consumer

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