-- Add migration script here
CREATE TABLE token_transfers (
    -- composite Primary keys
    signature TEXT NOT NULL,
    sender TEXT NOT NULL,
    receiver TEXT NOT NULL,
    mint TEXT NOT NULL,
    amount NUMERIC NOT NULL, -- handles u64 safely

    slot BIGINT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),

    -- IDEMPOTENT (prevents duplication)
    PRIMARY KEY (signature,sender,receiver,mint)
);

CREATE INDEX idx_sender ON token_transfers(sender);
CREATE INDEX idx_receiver ON token_transfers(receiver);
CREATE INDEX idx_slot ON token_transfers(slot);