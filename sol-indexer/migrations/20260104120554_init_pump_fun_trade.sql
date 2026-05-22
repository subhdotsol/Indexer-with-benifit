-- Add migration script here
CREATE TABLE pump_fun_trades (
    signature TEXT NOT NULL,
    slot BIGINT NOT NULL,
    block_time TIMESTAMPTZ NOT NULL,
    mint TEXT NOT NULL,
    is_buy BOOLEAN NOT NULL,
    user_address TEXT NOT NULL,
    token_amount NUMERIC NOT NULL,
    sol_amount NUMERIC NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (signature, mint)
);

CREATE INDEX idx_pump_fun_mint ON pump_fun_trades(mint);
CREATE INDEX idx_pump_fun_user ON pump_fun_trades(user_address);
