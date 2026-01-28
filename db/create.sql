CREATE TABLE raw_events (
    id BIGSERIAL PRIMARY KEY,
    event_type TEXT,
    payload JSONB,
    received_at TIMESTAMPTZ DEFAULT NOW()
);