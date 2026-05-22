CREATE TABLE indexer_state (
    id             TEXT PRIMARY KEY,
    last_slot      BIGINT NOT NULL,
    last_block_hash TEXT NOT NULL,
    updated_at     TIMESTAMPTZ DEFAULT NOW()
);

INSERT INTO indexer_state (id, last_slot, last_block_hash)
VALUES ('main_indexer', 0, '')
ON CONFLICT DO NOTHING;
