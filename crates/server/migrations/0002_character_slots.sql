-- Existing single-character accounts keep their hero and progress in slot 1.
ALTER TABLE characters DROP CONSTRAINT characters_account_id_key;
ALTER TABLE characters ADD COLUMN slot SMALLINT;
UPDATE characters SET slot = 1;
ALTER TABLE characters ALTER COLUMN slot SET NOT NULL;
ALTER TABLE characters ADD CONSTRAINT characters_slot_range CHECK (slot BETWEEN 1 AND 12);
ALTER TABLE characters ADD CONSTRAINT characters_account_slot_unique UNIQUE (account_id, slot);
