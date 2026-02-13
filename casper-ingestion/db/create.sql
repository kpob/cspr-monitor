CREATE TABLE raw_events (
    id BIGSERIAL PRIMARY KEY,
    event_type TEXT,
    payload JSONB,
    received_at TIMESTAMPTZ DEFAULT NOW()
);

ALTER TABLE raw_events
ADD COLUMN processed BOOLEAN DEFAULT FALSE;

ALTER TABLE raw_events
ADD COLUMN kafka_published BOOLEAN DEFAULT FALSE;

CREATE INDEX idx_raw_events_unprocessed
ON raw_events (id)
WHERE processed = FALSE;

CREATE TABLE tx_lifecycle (
    tx_hash TEXT PRIMARY KEY,

    -- intent (from TransactionAccepted)
    accepted_at TIMESTAMPTZ,
    sender TEXT,
    raw_accepted JSONB,

    -- effects (from TransactionProcessed)
    processed_at TIMESTAMPTZ,
    status TEXT,
    raw_processed JSONB
);

CREATE TABLE contract_usage (
    tx_hash TEXT PRIMARY KEY,
    contract TEXT,
    method TEXT,
    caller TEXT,
    block_number BIGINT,
    timestamp TIMESTAMPTZ
);
