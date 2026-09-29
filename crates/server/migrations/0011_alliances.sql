CREATE TABLE alliances (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL CHECK (char_length(name) BETWEEN 2 AND 24),
    leader_id BIGINT NOT NULL REFERENCES characters(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX alliances_name_unique ON alliances(lower(name));
CREATE TABLE alliance_members (
    character_id BIGINT PRIMARY KEY REFERENCES characters(id),
    alliance_id BIGINT NOT NULL REFERENCES alliances(id) ON DELETE CASCADE,
    officer BOOLEAN NOT NULL DEFAULT false,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(alliance_id, character_id)
);
-- A leader must remain a member; creation, transfer and dissolution are atomic.
ALTER TABLE alliances ADD CONSTRAINT alliance_leader_membership
    FOREIGN KEY(id, leader_id) REFERENCES alliance_members(alliance_id, character_id)
    DEFERRABLE INITIALLY DEFERRED;
CREATE TABLE alliance_invitations (
    id BIGSERIAL PRIMARY KEY,
    recipient_id BIGINT NOT NULL UNIQUE REFERENCES characters(id) ON DELETE CASCADE,
    alliance_id BIGINT NOT NULL REFERENCES alliances(id) ON DELETE CASCADE,
    inviter_id BIGINT NOT NULL REFERENCES characters(id),
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX alliance_invitation_expiry ON alliance_invitations(expires_at);
ALTER TABLE chat_messages DROP CONSTRAINT chat_messages_channel_check;
ALTER TABLE chat_messages ADD CONSTRAINT chat_messages_channel_check
    CHECK(channel IN ('general','local','group','alliance'));
-- Keep the historical audience even after dissolution; IDs are never reused.
ALTER TABLE chat_messages ADD COLUMN alliance_id BIGINT;
ALTER TABLE chat_messages ADD CONSTRAINT chat_alliance_audience
    CHECK ((channel = 'alliance') = (alliance_id IS NOT NULL));
