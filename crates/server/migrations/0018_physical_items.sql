-- Redesign stage 4: physical property. Every item stack is one row with a never-reused id and
-- exactly one location: a character's bag cell, worn slot or purse, a container cell, or the
-- ground. Moving property updates the location instead of creating copies for two owners.
-- Objects players place in the world are stored apart from the generated region geometry.
CREATE TABLE world_objects (
    id BIGSERIAL PRIMARY KEY,
    kind SMALLINT NOT NULL CHECK (kind BETWEEN 1 AND 255),
    x REAL NOT NULL,
    z REAL NOT NULL,
    yaw REAL NOT NULL DEFAULT 0,
    -- Code that a key with the same code opens; NULL is an unlocked object.
    lock_code BIGINT,
    -- Bumped on every change of the contents; stale requests carrying an older value fail.
    revision BIGINT NOT NULL DEFAULT 0,
    placed_by BIGINT REFERENCES characters(id) ON DELETE SET NULL,
    placed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX world_objects_position ON world_objects(x, z);

CREATE TABLE items (
    id BIGSERIAL PRIMARY KEY,
    kind SMALLINT NOT NULL CHECK (kind BETWEEN 1 AND 255),
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    -- Keys carry the code of the lock they open.
    lock_code BIGINT,
    place TEXT NOT NULL CHECK (place IN ('bag', 'worn', 'purse', 'container', 'ground')),
    character_id BIGINT REFERENCES characters(id) ON DELETE CASCADE,
    -- Bag or container cell; for worn items 0 = weapon, 1 = armour.
    cell INTEGER CHECK (cell BETWEEN 0 AND 65535),
    container_id BIGINT REFERENCES world_objects(id),
    x REAL,
    z REAL,
    CHECK (
        (place IN ('bag', 'worn') AND character_id IS NOT NULL AND cell IS NOT NULL
            AND container_id IS NULL AND x IS NULL AND z IS NULL)
        OR (place = 'purse' AND character_id IS NOT NULL AND cell IS NULL
            AND container_id IS NULL AND x IS NULL AND z IS NULL)
        OR (place = 'container' AND character_id IS NULL AND cell IS NOT NULL
            AND container_id IS NOT NULL AND x IS NULL AND z IS NULL)
        OR (place = 'ground' AND character_id IS NULL AND cell IS NULL
            AND container_id IS NULL AND x IS NOT NULL AND z IS NOT NULL)),
    CHECK (place <> 'worn' OR cell IN (0, 1)),
    -- Swaps move two stacks through each other's places inside one transaction.
    EXCLUDE USING btree (character_id WITH =, place WITH =, cell WITH =)
        WHERE (place IN ('bag', 'worn')) DEFERRABLE INITIALLY DEFERRED,
    EXCLUDE USING btree (character_id WITH =, kind WITH =)
        WHERE (place = 'purse') DEFERRABLE INITIALLY DEFERRED,
    EXCLUDE USING btree (container_id WITH =, cell WITH =)
        WHERE (place = 'container') DEFERRABLE INITIALLY DEFERRED
);
CREATE INDEX items_character ON items(character_id) WHERE character_id IS NOT NULL;
CREATE INDEX items_container ON items(container_id) WHERE container_id IS NOT NULL;
CREATE INDEX items_ground ON items(x, z) WHERE place = 'ground';

-- The old inventory (kind list, loot counts, copy-token bag cells, coins/gold stats) is
-- converted once by the server (`store::convert_legacy_items`) after the stage-2 conversion.
-- The archive row keeps the exact input; the old columns are emptied by the converter so
-- they never become a second source of property.
INSERT INTO legacy_character_archive(character_id, migration, data)
SELECT id, '0018_physical_items', jsonb_build_object(
    'stats', stats,
    'inventory', inventory,
    'loot', loot)
FROM characters;

-- Property recorded by 0017 is released into the world by the same converter: where it went
-- is kept next to the record.
ALTER TABLE legacy_property ADD COLUMN released_to JSONB
    CHECK (released_to IS NULL OR jsonb_typeof(released_to) = 'object');
-- Released records are history: their owner may later be erased or dissolved. Unreleased
-- records still block that, now through the owner check instead of the bare foreign keys.
DO $$
DECLARE c record;
BEGIN
    FOR c IN SELECT conname FROM pg_constraint
        WHERE conrelid = 'legacy_property'::regclass
          AND (contype = 'f' OR (contype = 'c' AND pg_get_constraintdef(oid) LIKE '%num_nonnulls%'))
    LOOP
        EXECUTE format('ALTER TABLE legacy_property DROP CONSTRAINT %I', c.conname);
    END LOOP;
END $$;
ALTER TABLE legacy_property
    ADD CONSTRAINT legacy_property_character_fk FOREIGN KEY (character_id)
        REFERENCES characters(id) ON DELETE SET NULL,
    ADD CONSTRAINT legacy_property_alliance_fk FOREIGN KEY (alliance_id)
        REFERENCES alliances(id) ON DELETE SET NULL,
    ADD CONSTRAINT legacy_property_owner
        CHECK (released_at IS NOT NULL OR num_nonnulls(character_id, alliance_id) = 1);
