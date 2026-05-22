CREATE TABLE jupiter_swaps (
    signature       TEXT PRIMARY KEY,
    slot            BIGINT NOT NULL,
    block_time      TIMESTAMPTZ NOT NULL,
    signer          TEXT NOT NULL,
    amm_pool        TEXT,
    mint_in         TEXT NOT NULL,
    mint_out        TEXT NOT NULL,
    amount_in       DECIMAL(20,0) NOT NULL,
    amount_out      DECIMAL(20,0) NOT NULL,
    slippage_bps    INTEGER,
    platform_fee_bps INTEGER,
    route_plan      JSONB,
    created_at      TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_jup_signer ON jupiter_swaps(signer);
CREATE INDEX idx_jup_mints  ON jupiter_swaps(mint_in, mint_out);
CREATE INDEX idx_jup_slot   ON jupiter_swaps(slot);
