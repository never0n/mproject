ALTER TABLE characters ADD COLUMN inventory JSONB NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE characters ADD CONSTRAINT inventory_is_array CHECK (jsonb_typeof(inventory)='array');
-- One starting kit per existing hero; this migration never runs twice.
UPDATE characters SET inventory=jsonb_build_array(
    jsonb_build_object('kind', CASE WHEN class=1 THEN 1 ELSE 2 END, 'equipped', false),
    jsonb_build_object('kind', 3, 'equipped', false));
