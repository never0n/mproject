-- Redesign stage 2: every character is a human without class.
-- The old race/class and the state they shaped are archived; the server converts each archived
-- character once on startup (store::convert_legacy_characters) and stamps converted_at.
ALTER TABLE legacy_character_archive ADD COLUMN converted_at TIMESTAMPTZ;
UPDATE legacy_character_archive SET converted_at = archived_at;

INSERT INTO legacy_character_archive(character_id, migration, data)
SELECT id, '0016_human_characters', jsonb_build_object(
    'race', race,
    'class', class,
    'stats', stats,
    'inventory', inventory,
    'loot', loot)
FROM characters;

ALTER TABLE characters DROP COLUMN race, DROP COLUMN class;
