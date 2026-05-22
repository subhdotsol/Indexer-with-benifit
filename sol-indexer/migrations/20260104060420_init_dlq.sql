-- Add migration script here
CREATE TABLE transaction_dlq(
    signature TEXT PRIMARY KEY,
    slot BIGINT NOT NULL,
    parser_name TEXT NOT NULL,
    error_msg TEXT,
    -- raw tx, so we can restruct the whole tx
    tx_data JSONB NOT NULL,
    status TEXT DEFAULT 'pending', -- 'pending','resolved','ignored'
    retry_count INT DEFAULT 0,
    created_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_dlq_status ON transaction_dlq(status);