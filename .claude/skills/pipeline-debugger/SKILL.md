---
name: pipeline-debugger
description: >
  Debug transaction flow through the casper-monitor pipeline. Use when the user asks about
  missing transactions, correlation failures, stuck events, consumer lag, or wants to trace
  a tx_hash through the system. Also trigger on: "where is my transaction", "why didn't it
  correlate", "check pipeline status", or any debugging of the ingestion→router→consumer flow.
---

# Pipeline Debugger

Trace a transaction's journey through the casper-monitor pipeline and identify where it got stuck.

## Pipeline stages

```
Casper Node (SSE)
  → casper-ingestion → raw_events (PostgreSQL) + raw.chain_events (Kafka)
    → casper-event-router → tx_lifecycle (PostgreSQL) + enriched.chain_events (Kafka)
      → identifier matching → apps.* topics (Kafka)
        → downstream consumers (delta-filter, exchange-monitor, whale-activity)
```

## Step 1: Get the tx_hash

Ask the user for the transaction hash (64-char hex string). If they don't have it, help them find it:
- Check recent `tx_lifecycle` rows: `SELECT tx_hash, accepted_at, status FROM tx_lifecycle ORDER BY accepted_at DESC LIMIT 10`
- Check recent `raw_events`: `SELECT id, event_type, received_at FROM raw_events ORDER BY received_at DESC LIMIT 10`

## Step 2: Check each stage

### Stage A — Ingestion (raw_events table)

```sql
SELECT id, event_type, received_at, kafka_published, processed
FROM raw_events
WHERE payload::text LIKE '%<TX_HASH>%'
ORDER BY received_at;
```

**Expected**: Two rows — one `TransactionAccepted`, one `TransactionProcessed`.
- 0 rows: Transaction never reached ingestion. Check SSE connection and ingestion container logs.
- 1 row (Accepted only): TransactionProcessed not yet received, or ingestion filtered it.
- `kafka_published = false`: Kafka publish failed; event is DB-only (non-fatal fallback).

### Stage B — Kafka raw topic

Check ingestion logs for the tx_hash:
```bash
docker logs casper-monitor-ingestion-1 2>&1 | grep <TX_HASH> | tail -5
```

Or consume the topic (slow, use only if logs are rotated):
```bash
docker exec casper-monitor-kafka-1 kafka-console-consumer.sh \
  --bootstrap-server localhost:29092 --topic raw.chain_events --from-beginning \
  --max-messages 1000 2>/dev/null | grep <TX_HASH>
```

### Stage C — Correlation (tx_lifecycle table)

```sql
SELECT tx_hash, accepted_at, sender, processed_at, status
FROM tx_lifecycle
WHERE tx_hash = '<TX_HASH>';
```

**Expected**: One row with both `accepted_at` and `processed_at` set.
- Row missing: Event-router didn't receive or parse the raw event. Check router logs.
- `accepted_at` set, `processed_at` NULL: TransactionProcessed not yet correlated. Either:
  - Not yet received (check Stage A)
  - DashMap timeout exceeded (5 minutes between accepted and processed)
  - Parse error in router (check logs)
- `processed_at` set, `accepted_at` NULL: TransactionAccepted arrived after processed (unusual but handled by upsert).

### Stage D — Identifier matching

Check event-router logs:
```bash
docker logs casper-monitor-event-router-1 2>&1 | grep <TX_HASH> | tail -10
```

Look for:
- `"Publishing enriched transaction"` — correlation succeeded
- `"Contract match"` or `"Exchange match"` — identifier matched, routed to apps.* topic
- `"No identifier match"` — transaction went to `apps.unclassified`

### Stage E — Consumer lag

```bash
docker exec casper-monitor-kafka-1 kafka-consumer-groups.sh \
  --bootstrap-server localhost:29092 --describe --group event-router
```

If LAG column shows high numbers, the router is falling behind. Check for processing errors or resource constraints.

## Step 3: Common failure modes

| Symptom | Likely cause | Fix |
|---------|-------------|-----|
| 0 rows in raw_events | SSE stream disconnected | Check ingestion logs for reconnect errors |
| kafka_published = false | Kafka broker unreachable | Check Kafka container health |
| tx_lifecycle row missing | Router consumer lag or crash | Check router logs + consumer group lag |
| accepted_at set, processed_at NULL | Correlation timeout (>5 min) | Check if TransactionProcessed arrived; may be nctl delay |
| No identifier match | Sender/contract not in config | Check `casper-event-router/config/apps_config.json` |
| v1 vs v2 parse error | Wrong variant detection | See casper-tx-analyzer skill for schema details |
| Deploy args extraction wrong | Session JSON is variant-wrapped | `session.Transfer.args` not `session.args` (known past bug) |

## Step 4: Quick health check (no tx_hash needed)

If the user just wants overall pipeline health:

```sql
-- Recent ingestion activity
SELECT event_type, COUNT(*), MAX(received_at) as latest
FROM raw_events
WHERE received_at > NOW() - INTERVAL '5 minutes'
GROUP BY event_type;

-- Recent correlation activity
SELECT status, COUNT(*), MAX(processed_at) as latest
FROM tx_lifecycle
WHERE processed_at > NOW() - INTERVAL '5 minutes'
GROUP BY status;

-- Uncorrelated transactions (accepted but not processed)
SELECT COUNT(*) as pending
FROM tx_lifecycle
WHERE processed_at IS NULL AND accepted_at > NOW() - INTERVAL '10 minutes';
```

Then check consumer lag:
```bash
docker exec casper-monitor-kafka-1 kafka-consumer-groups.sh \
  --bootstrap-server localhost:29092 --describe --group event-router
```
