ALTER TABLE character_mail
    ADD COLUMN attachment_returned BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN return_of BIGINT UNIQUE REFERENCES character_mail(id),
    ADD CONSTRAINT mail_return_state CHECK (
        NOT attachment_returned OR (attachment_kind IS NOT NULL AND NOT attachment_claimed AND return_of IS NULL)),
    ADD CONSTRAINT mail_return_free CHECK (return_of IS NULL OR (attachment_kind IS NOT NULL AND attachment_price=0));
-- Returned letters use their own identity and cannot collide with player drafts.
ALTER TABLE character_mail DROP CONSTRAINT character_mail_sender_id_draft_key;
CREATE UNIQUE INDEX character_mail_player_draft ON character_mail(sender_id,draft) WHERE return_of IS NULL;
