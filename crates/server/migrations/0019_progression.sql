-- Redesign stage 5: the old skill-point currency stops being a source of development.
-- The archive keeps the exact stats, including unspent SP. The server converter removes SP
-- from the live sheet, keeps already purchased skills and grants talent points from level.
INSERT INTO legacy_character_archive(character_id, migration, data)
SELECT id, '0019_progression', jsonb_build_object('stats', stats)
FROM characters;
