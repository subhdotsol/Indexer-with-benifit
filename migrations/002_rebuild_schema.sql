-- Rebuild schema to match new domain models with composite PKs and NUMERIC types

-- Drop old tables that used UUID PKs and BIGINT amounts
DROP TABLE IF EXISTS pumpfun_swaps;
DROP TABLE IF EXISTS jupiter_swaps;
DROP TABLE IF EXISTS raydium_swaps;
DROP TABLE IF EXISTS token_transfers;

-- token_transfers: composite PK prevents duplicate (sig, sender, receiver, mint) events
CREATE TABLE token_transfers (
    signature TEXT NOT NULL,
    sender    TEXT NOT NULL,
    receiver  TEXT NOT NULL,
    mint      TEXT NOT NULL,
    amount    NUMERIC NOT NULL,
    slot      BIGINT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (signature, sender, receiver, mint)
);

CREATE INDEX idx_tt_sender   ON token_transfers(sender);
CREATE INDEX idx_tt_receiver ON token_transfers(receiver);
CREATE INDEX idx_tt_slot     ON token_transfers(slot);
