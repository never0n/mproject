-- A higher beam stands only on the beam under it. Poor soil grows slower.
-- Existing plots stay fertile: fertile ground is not rare, and the starting farms were already sown ground.
ALTER TABLE blocks
    ADD COLUMN level SMALLINT NOT NULL DEFAULT 0 CHECK (level BETWEEN 0 AND 3);

ALTER TABLE plots
    ADD COLUMN fertility SMALLINT NOT NULL DEFAULT 3 CHECK (fertility BETWEEN 1 AND 3);
