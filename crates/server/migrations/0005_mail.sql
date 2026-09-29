CREATE TABLE character_mail (
    id BIGSERIAL PRIMARY KEY,
    sender_id BIGINT NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    recipient_id BIGINT NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    subject TEXT NOT NULL CHECK (char_length(subject) BETWEEN 1 AND 128),
    body TEXT NOT NULL CHECK (char_length(body) BETWEEN 1 AND 1024),
    draft TEXT NOT NULL CHECK (draft ~ '^[0-9a-f]{32}$'),
    is_read BOOLEAN NOT NULL DEFAULT false,
    sender_deleted BOOLEAN NOT NULL DEFAULT false,
    recipient_deleted BOOLEAN NOT NULL DEFAULT false,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (sender_id <> recipient_id),
    UNIQUE (sender_id, draft)
);
CREATE INDEX character_mail_inbox ON character_mail (recipient_id, id DESC)
    WHERE NOT recipient_deleted;
CREATE INDEX character_mail_sent ON character_mail (sender_id, id DESC)
    WHERE NOT sender_deleted;
CREATE INDEX character_mail_unread ON character_mail (recipient_id)
    WHERE NOT recipient_deleted AND NOT is_read;
CREATE INDEX character_mail_rate ON character_mail (sender_id, sent_at DESC);
