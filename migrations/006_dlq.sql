CREATE TABLE transaction_dlq (
    signature   TEXT PRIMARY KEY,
    slot        BIGINT NOT NULL,
    parser_name TEXT NOT NULL,
    error_msg   TEXT,
    tx_data     JSONB NOT NULL,
    status      TEXT DEFAULT 'pending',
    retry_count INT DEFAULT 0,
    created_at  TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_dlq_status ON transaction_dlq(status);
