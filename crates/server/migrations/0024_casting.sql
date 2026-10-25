-- A poured blank keeps its own mass and shape. Old stacks have no mass_mg and keep the catalogue weight.
-- A three-byte blade plan in character stats is not converted: it is not a physical casting.
ALTER TABLE items
    ADD COLUMN mass_mg BIGINT CHECK (mass_mg IS NULL OR mass_mg > 0);

CREATE TABLE blueprints (
    id BIGSERIAL PRIMARY KEY,
    character_id BIGINT NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    solver_version INTEGER NOT NULL,
    material_id TEXT NOT NULL,
    material_version INTEGER NOT NULL,
    time_scale INTEGER NOT NULL,
    outline JSONB NOT NULL,
    pourable BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One foundry session per character. Reserved metal is the charge_mg, not a bag stack.
CREATE TABLE craft_sessions (
    character_id BIGINT PRIMARY KEY REFERENCES characters(id) ON DELETE CASCADE,
    blueprint_id BIGINT REFERENCES blueprints(id),
    revision BIGINT NOT NULL DEFAULT 0,
    phase TEXT NOT NULL CHECK (phase IN ('draft', 'charged', 'hot', 'pouring', 'poured')),
    charge_mg BIGINT NOT NULL DEFAULT 0 CHECK (charge_mg >= 0),
    temperature_c DOUBLE PRECISION,
    physical_s DOUBLE PRECISION NOT NULL DEFAULT 0,
    gate_mm2 INTEGER,
    ideal_feed BOOLEAN NOT NULL DEFAULT false,
    checkpoint JSONB,
    result_item BIGINT REFERENCES items(id)
);

CREATE TABLE item_bodies (
    item_id BIGINT PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,
    solver_version INTEGER NOT NULL,
    material_id TEXT NOT NULL,
    material_version INTEGER NOT NULL,
    mass_mg BIGINT NOT NULL CHECK (mass_mg > 0),
    feed_status TEXT NOT NULL,
    geometry JSONB NOT NULL,
    defects JSONB NOT NULL
);
