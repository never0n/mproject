ALTER TABLE characters ADD COLUMN loot JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE characters ADD CONSTRAINT loot_is_object CHECK (jsonb_typeof(loot)='object');
