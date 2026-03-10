CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_raw_events_received_at
ON raw_events (received_at);
