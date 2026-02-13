default:
  @just --list

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
  echo "✓ librdkafka installed successfully"

init-db:
    psql -d ingest_dev -f db/create.sql

ingest-test:
    psql -d ingest_test -f db/create.sql
    cargo test


set dotenv-load := true

# Możesz nadpisać w .env, ale te wartości są OK dla Twojego setupu
KAFKA_CONTAINER := env_var_or_default("KAFKA_CONTAINER", "kafka")
BOOTSTRAP       := env_var_or_default("KAFKA_BOOTSTRAP", "127.0.0.1:9092")


up:
  docker compose up -d

down:
  docker compose down

restart:
  docker compose down
  docker compose up -d

logs:
  docker compose logs -f

kafka-logs:
  docker logs -f {{KAFKA_CONTAINER}}

ps:
  docker compose ps

# --- Kafka CLI helpers ---

topics:
  docker exec {{KAFKA_CONTAINER}} /opt/kafka/bin/kafka-topics.sh --bootstrap-server {{BOOTSTRAP}} --list

topic-create name partitions='6':
  docker exec {{KAFKA_CONTAINER}} /opt/kafka/bin/kafka-topics.sh \
    --bootstrap-server {{BOOTSTRAP}} \
    --create \
    --topic {{name}} \
    --partitions {{partitions}} \
    --replication-factor 1

topic-delete name:
  docker exec {{KAFKA_CONTAINER}} /opt/kafka/bin/kafka-topics.sh \
    --bootstrap-server {{BOOTSTRAP}} \
    --delete \
    --topic {{name}}

topic-describe name:
  docker exec {{KAFKA_CONTAINER}} /opt/kafka/bin/kafka-topics.sh \
    --bootstrap-server {{BOOTSTRAP}} \
    --describe \
    --topic {{name}}

produce topic:
  docker exec -it {{KAFKA_CONTAINER}} /opt/kafka/bin/kafka-console-producer.sh \
    --bootstrap-server {{BOOTSTRAP}} \
    --topic {{topic}}

consume topic:
  docker exec -it {{KAFKA_CONTAINER}} /opt/kafka/bin/kafka-console-consumer.sh \
    --bootstrap-server {{BOOTSTRAP}} \
    --topic {{topic}} \
    --from-beginning

consume-latest topic:
  docker exec -it {{KAFKA_CONTAINER}} /opt/kafka/bin/kafka-console-consumer.sh \
    --bootstrap-server {{BOOTSTRAP}} \
    --topic {{topic}}

groups:
  docker exec {{KAFKA_CONTAINER}} /opt/kafka/bin/kafka-consumer-groups.sh \
    --bootstrap-server {{BOOTSTRAP}} \
    --list

group-describe group:
  docker exec {{KAFKA_CONTAINER}} /opt/kafka/bin/kafka-consumer-groups.sh \
    --bootstrap-server {{BOOTSTRAP}} \
    --describe \
    --group {{group}}

# --- Convenience: create your project topics ---
init-topics:
  just topic-create raw.chain_events 12
  just topic-create enriched.chain_events 12
  just topic-create signals.arbitrage 3
