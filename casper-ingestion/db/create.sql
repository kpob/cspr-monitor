CREATE TABLE raw_events (
    id BIGSERIAL PRIMARY KEY,
    event_type TEXT,
    payload JSONB,
    received_at TIMESTAMPTZ DEFAULT NOW()
);

ALTER TABLE raw_events
ADD COLUMN processed BOOLEAN DEFAULT FALSE;

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
