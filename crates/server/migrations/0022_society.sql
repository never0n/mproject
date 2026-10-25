-- Stage 9. Several organisation types, private recognition, physical letters.
-- An old alliance becomes a guild. Membership of a past identity does not occupy a new one,
-- except a leader row, which stays until leadership is passed on.
ALTER TABLE alliances
    ADD COLUMN kind SMALLINT NOT NULL DEFAULT 2 CHECK (kind BETWEEN 1 AND 3);

ALTER TABLE alliance_members DROP CONSTRAINT alliance_members_pkey;
ALTER TABLE alliance_members ADD PRIMARY KEY (character_id, alliance_id);
ALTER TABLE alliance_members
    ADD COLUMN kind SMALLINT NOT NULL DEFAULT 2 CHECK (kind BETWEEN 1 AND 3),
    ADD COLUMN identity DOUBLE PRECISION NOT NULL DEFAULT 1;
CREATE UNIQUE INDEX alliance_members_one_kind ON alliance_members (character_id, kind);

ALTER TABLE alliance_invitations DROP CONSTRAINT alliance_invitations_recipient_id_key;
ALTER TABLE alliance_invitations
    ADD COLUMN kind SMALLINT NOT NULL DEFAULT 2 CHECK (kind BETWEEN 1 AND 3);
CREATE UNIQUE INDEX alliance_invitations_one_kind ON alliance_invitations (recipient_id, kind);

ALTER TABLE characters
    ADD COLUMN presented TEXT NOT NULL DEFAULT '',
    ADD COLUMN face_hidden BOOLEAN NOT NULL DEFAULT false;

CREATE TABLE acquaintances (
    owner_id BIGINT NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    person_id BIGINT NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    note TEXT NOT NULL CHECK (char_length(note) BETWEEN 1 AND 80),
    identity DOUBLE PRECISION NOT NULL,
    PRIMARY KEY (owner_id, person_id)
);

CREATE TABLE documents (
    item_id BIGINT PRIMARY KEY REFERENCES items (id) ON DELETE CASCADE,
    body TEXT NOT NULL CHECK (char_length(body) BETWEEN 1 AND 280),
    author_id BIGINT REFERENCES characters (id) ON DELETE SET NULL,
    identity DOUBLE PRECISION NOT NULL
);

CREATE TABLE couriers (
    item_id BIGINT PRIMARY KEY REFERENCES items (id) ON DELETE CASCADE,
    dest_x REAL NOT NULL,
    dest_z REAL NOT NULL
);
