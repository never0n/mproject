-- Centre of mass along the drawing and inertia about that centre. Old blanks leave these empty.
ALTER TABLE item_bodies
    ADD COLUMN center_m DOUBLE PRECISION,
    ADD COLUMN inertia_kg_m2 DOUBLE PRECISION;
