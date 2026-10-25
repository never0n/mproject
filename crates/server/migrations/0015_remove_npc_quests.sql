-- Redesign stage 1: the merchant's quest chain and the shop-discount passive are removed.
-- Removed per-character values are archived outside the active stats before they are deleted.
CREATE TABLE legacy_character_archive (
    id BIGSERIAL PRIMARY KEY,
    character_id BIGINT NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    migration TEXT NOT NULL,
    data JSONB NOT NULL CHECK (jsonb_typeof(data)='object'),
    archived_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (character_id, migration)
);

INSERT INTO legacy_character_archive(character_id, migration, data)
SELECT id, '0015_remove_npc_quests', jsonb_strip_nulls(jsonb_build_object(
    'quest_stage', stats->'quest_stage',
    'quest_kills', stats->'quest_kills',
    'quest_crafted', stats->'quest_crafted',
    'extra_skills', stats->'extra_skills',
    'sp', stats->'sp'))
FROM characters
WHERE stats ?| ARRAY['quest_stage','quest_kills','quest_crafted']
   OR (coalesce((stats->>'extra_skills')::numeric, 0)::int & 16) <> 0;

UPDATE characters SET stats = stats - 'quest_stage' - 'quest_kills' - 'quest_crafted'
WHERE stats ?| ARRAY['quest_stage','quest_kills','quest_crafted'];

-- Skill 7 ("Торгаш", bit 16 of extra_skills) only discounted the NPC shop; its 5 SP are refunded.
UPDATE characters SET stats = stats || jsonb_build_object(
    'extra_skills', (stats->>'extra_skills')::numeric::int & ~16,
    'sp', coalesce((stats->>'sp')::numeric, 0) + 5)
WHERE (coalesce((stats->>'extra_skills')::numeric, 0)::int & 16) <> 0;
