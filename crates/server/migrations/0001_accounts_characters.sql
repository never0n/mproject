CREATE TABLE accounts (
    id BIGSERIAL PRIMARY KEY,
    login TEXT NOT NULL CHECK (length(login) BETWEEN 3 AND 16),
    password_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX accounts_login_unique ON accounts (lower(login));
CREATE TABLE characters (
    id BIGSERIAL PRIMARY KEY,
    account_id BIGINT NOT NULL UNIQUE REFERENCES accounts(id) ON DELETE CASCADE,
    nickname TEXT NOT NULL CHECK (char_length(nickname) BETWEEN 2 AND 16),
    race SMALLINT NOT NULL CHECK (race BETWEEN 1 AND 6),
    class SMALLINT NOT NULL CHECK (class BETWEEN 1 AND 2),
    gender SMALLINT NOT NULL CHECK (gender BETWEEN 1 AND 2),
    stats JSONB NOT NULL,
    x REAL NOT NULL, z REAL NOT NULL,
    yaw REAL NOT NULL DEFAULT 0,
    saved_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX characters_nickname_unique ON characters (lower(nickname));
