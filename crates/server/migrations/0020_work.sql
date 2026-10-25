-- Stage 7. Physical sources, one crop plot and a price on a player stall.
-- A restart does not refill a depleted node or a harvested plot: rows are only
-- inserted when the table is still empty.
CREATE TABLE resource_nodes (
    id BIGSERIAL PRIMARY KEY,
    product SMALLINT NOT NULL CHECK (product BETWEEN 1 AND 255),
    x REAL NOT NULL,
    z REAL NOT NULL,
    remaining INTEGER NOT NULL CHECK (remaining >= 0)
);
CREATE TABLE plots (
    id BIGSERIAL PRIMARY KEY,
    x REAL NOT NULL,
    z REAL NOT NULL,
    stage SMALLINT NOT NULL DEFAULT 0 CHECK (stage BETWEEN 0 AND 2),
    ready_at TIMESTAMPTZ
);
CREATE TABLE stall_prices (
    object_id BIGINT PRIMARY KEY REFERENCES world_objects (id) ON DELETE CASCADE,
    price BIGINT NOT NULL CHECK (price >= 0)
);
