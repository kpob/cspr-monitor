INSERT INTO tx_lifecycle (
    tx_hash,
    accepted_at,
    sender,
    payment,
    raw_accepted
)
VALUES (...)
ON CONFLICT (tx_hash)
DO UPDATE SET
    accepted_at = EXCLUDED.accepted_at,
    sender = EXCLUDED.sender,
    payment = EXCLUDED.payment,
    raw_accepted = EXCLUDED.raw_accepted;

INSERT INTO tx_lifecycle (
    tx_hash,
    processed_at,
    status,
    raw_processed
)
VALUES (...)
ON CONFLICT (tx_hash)
DO UPDATE SET
    processed_at = EXCLUDED.processed_at,
    status = EXCLUDED.status,
    raw_processed = EXCLUDED.raw_processed;
