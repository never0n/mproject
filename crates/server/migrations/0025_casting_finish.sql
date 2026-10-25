-- Seconds of lumped cooling already applied to a blank. Server downtime does not add to this.
ALTER TABLE item_bodies
    ADD COLUMN physical_s DOUBLE PRECISION NOT NULL DEFAULT 0;
