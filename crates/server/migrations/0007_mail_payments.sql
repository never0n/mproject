ALTER TABLE character_mail ADD COLUMN attachment_price INTEGER NOT NULL DEFAULT 0
    CHECK (attachment_price BETWEEN 0 AND 1000000);
ALTER TABLE character_mail ADD CONSTRAINT mail_price_requires_attachment
    CHECK (attachment_price=0 OR attachment_kind IS NOT NULL);
CREATE TABLE mail_payments (
    mail_id BIGINT PRIMARY KEY REFERENCES character_mail(id),
    owner_id BIGINT NOT NULL REFERENCES characters(id),
    amount BIGINT NOT NULL CHECK (amount > 0 AND amount <= 1000000),
    settled BOOLEAN NOT NULL DEFAULT false
);
CREATE INDEX mail_payments_pending ON mail_payments(owner_id) WHERE NOT settled;
