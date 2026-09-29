ALTER TABLE character_mail
    ADD COLUMN attachment_kind SMALLINT,
    ADD COLUMN attachment_quantity INTEGER,
    ADD COLUMN attachment_token BIGINT,
    ADD COLUMN attachment_claimed BOOLEAN NOT NULL DEFAULT false,
    ADD CONSTRAINT character_mail_attachment_valid CHECK (
        (attachment_kind IS NULL AND attachment_quantity IS NULL AND attachment_token IS NULL AND NOT attachment_claimed)
        OR (attachment_kind IS NOT NULL AND attachment_kind BETWEEN 1 AND 10
            AND attachment_quantity IS NOT NULL AND attachment_quantity BETWEEN 1 AND 9999
            AND attachment_token IS NOT NULL AND attachment_token > 0
            AND (attachment_kind > 6 OR attachment_quantity = 1))
    );
