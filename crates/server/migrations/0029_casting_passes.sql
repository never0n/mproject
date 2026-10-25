-- A blank remembers the blueprint it was cast from and the passes that removed its metal.
-- Older blanks have no blueprint link; their history starts empty. A blueprint goes with
-- its author, while the blank may already belong to someone else and stays.
ALTER TABLE item_bodies
    ADD COLUMN blueprint_id BIGINT REFERENCES blueprints(id) ON DELETE SET NULL,
    ADD COLUMN passes JSONB NOT NULL DEFAULT '[]'::jsonb;
