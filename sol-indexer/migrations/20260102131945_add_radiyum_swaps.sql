-- Add migration script here
CREATE TABLE IF NOT EXISTS raydium_swaps (
    signature TEXT NOT NULL,
    amm_pool TEXT NOT NULL,
    sender TEXT NOT NULL,
    amount_in NUMERIC(20,0) NOT NULL,
    min_amount_out NUMERIC(20,0) NOT NULL,
    amount_received NUMERIC(20,0) NOT NULL,
    mint_source TEXT NOT NULL,
    mint_destination TEXT NOT NULL,
    slot BIGINT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    -- Composite primary 
    PRIMARY KEY (signature, amm_pool)
);

CREATE INDEX idx_raydium_swaps_sender ON raydium_swaps(sender);
CREATE INDEX idx_raydium_swaps_pool ON raydium_swaps(amm_pool);
CREATE INDEX idx_raydium_swaps_mints ON raydium_swaps(mint_source,mint_destination);