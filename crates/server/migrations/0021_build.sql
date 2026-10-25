-- Stage 8. Placed timber blocks and a one-time assignment of the starting plots.
-- A restart does not rebuild a destroyed block and does not grant the plots again.
ALTER TABLE plots
    ADD COLUMN owner_id BIGINT REFERENCES characters (id) ON DELETE SET NULL;

CREATE TABLE blocks (
    id BIGSERIAL PRIMARY KEY,
    x REAL NOT NULL,
    z REAL NOT NULL,
    integrity SMALLINT NOT NULL CHECK (integrity BETWEEN 1 AND 8),
    placed_by BIGINT REFERENCES characters (id) ON DELETE SET NULL
);

CREATE TABLE world_grants (
    name TEXT PRIMARY KEY,
    character_id BIGINT REFERENCES characters (id) ON DELETE SET NULL,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
