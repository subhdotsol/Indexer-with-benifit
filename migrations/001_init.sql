-- Token Transfers
CREATE TABLE IF NOT EXISTS token_transfers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    signature TEXT NOT NULL UNIQUE,
    slot BIGINT NOT NULL,
    from_address TEXT NOT NULL,
    to_address TEXT NOT NULL,
    amount BIGINT NOT NULL,
    mint TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Raydium Swaps
CREATE TABLE IF NOT EXISTS raydium_swaps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    signature TEXT NOT NULL UNIQUE,
    slot BIGINT NOT NULL,
    amm_pool TEXT NOT NULL,
    signer TEXT NOT NULL,
    amount_in BIGINT NOT NULL,
    min_amount_out BIGINT NOT NULL,
    amount_received BIGINT NOT NULL,
    mint_source TEXT NOT NULL,
    mint_destination TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Jupiter Swaps
CREATE TABLE IF NOT EXISTS jupiter_swaps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    signature TEXT NOT NULL UNIQUE,
    slot BIGINT NOT NULL,
    signer TEXT NOT NULL,
    amm_pool TEXT NOT NULL,
    mint_in TEXT NOT NULL,
    mint_out TEXT NOT NULL,
    amount_in BIGINT NOT NULL,
    amount_out BIGINT NOT NULL,
    slippage_bps SMALLINT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- PumpFun Swaps
CREATE TABLE IF NOT EXISTS pumpfun_swaps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    signature TEXT NOT NULL UNIQUE,
    slot BIGINT NOT NULL,
    signer TEXT NOT NULL,
    mint TEXT NOT NULL,
    is_buy BOOLEAN NOT NULL,
    sol_amount BIGINT NOT NULL,
    token_amount BIGINT NOT NULL,
    bonding_curve TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes for common queries
CREATE INDEX idx_token_transfers_slot ON token_transfers(slot);
CREATE INDEX idx_raydium_swaps_slot ON raydium_swaps(slot);
CREATE INDEX idx_jupiter_swaps_slot ON jupiter_swaps(slot);
CREATE INDEX idx_pumpfun_swaps_slot ON pumpfun_swaps(slot);