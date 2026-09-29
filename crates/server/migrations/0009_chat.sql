CREATE TABLE chat_messages (
    id BIGSERIAL PRIMARY KEY,
    sender_id BIGINT NOT NULL REFERENCES characters(id),
    channel TEXT NOT NULL CHECK(channel IN ('general','local')),
    body TEXT NOT NULL CHECK(char_length(body) BETWEEN 1 AND 280),
    sent_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX chat_rate ON chat_messages(sender_id,sent_at DESC);
CREATE TABLE chat_ignores (
    owner_id BIGINT NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    blocked_id BIGINT NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    PRIMARY KEY(owner_id,blocked_id), CHECK(owner_id<>blocked_id)
);
CREATE TABLE chat_reports (
    reporter_id BIGINT NOT NULL REFERENCES characters(id),
    message_id BIGINT NOT NULL REFERENCES chat_messages(id),
    reported_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY(reporter_id,message_id)
);
CREATE TABLE chat_mutes (
    character_id BIGINT PRIMARY KEY REFERENCES characters(id) ON DELETE CASCADE,
    until_at TIMESTAMPTZ NOT NULL,
    reason TEXT NOT NULL DEFAULT ''
);
