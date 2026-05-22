-- Add migration script here

CREATE TABLE indexer_state (
    id TEXT PRIMARY KEY, -- supports multiple indexer later
    last_slot BIGINT NOT NULL,
    last_block_hash TEXT NOT NULL, --for reorg detection
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- initializing with a base state
INSERT INTO indexer_state (id,last_slot,last_block_hash)
VALUES ('main_indexer',0,'')
ON CONFLICT DO NOTHING;