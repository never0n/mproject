ALTER TABLE alliances ADD COLUMN storage_revision BIGINT NOT NULL DEFAULT 0
    CHECK (storage_revision >= 0);

-- A populated warehouse must be emptied before its alliance can be dissolved.
CREATE TABLE alliance_storage (
    id BIGSERIAL PRIMARY KEY,
    alliance_id BIGINT NOT NULL REFERENCES alliances(id),
    slot INTEGER NOT NULL CHECK (slot BETWEEN 0 AND 49),
    kind SMALLINT NOT NULL CHECK (kind BETWEEN 1 AND 10),
    quantity INTEGER NOT NULL CHECK (quantity BETWEEN 1 AND 9999),
    CHECK (kind > 6 OR quantity = 1),
    UNIQUE (alliance_id, slot)
);
CREATE UNIQUE INDEX alliance_storage_stack ON alliance_storage(alliance_id, kind) WHERE kind > 6;

CREATE TABLE alliance_storage_events (
    id BIGSERIAL PRIMARY KEY,
    alliance_id BIGINT NOT NULL REFERENCES alliances(id) ON DELETE CASCADE,
    character_id BIGINT NOT NULL REFERENCES characters(id),
    nickname TEXT NOT NULL,
    deposit BOOLEAN NOT NULL,
    kind SMALLINT NOT NULL CHECK (kind BETWEEN 1 AND 10),
    quantity INTEGER NOT NULL CHECK (quantity BETWEEN 1 AND 9999),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX alliance_storage_events_recent ON alliance_storage_events(alliance_id, id DESC);
