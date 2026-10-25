-- Redesign stage 3: global mail, the alliance warehouse and exclusive professions are removed.
-- Their tables stay as frozen history; no server code reads or writes them any more.
-- Everything they still held for someone is recorded once in legacy_property and kept there
-- until stage 4 moves it into the physical world. While a record is unreleased its owner
-- cannot be erased or dissolved (FKs without cascade).
CREATE TABLE legacy_property (
    id BIGSERIAL PRIMARY KEY,
    source TEXT NOT NULL CHECK (source IN ('mail_attachment', 'mail_payment', 'alliance_storage')),
    source_id BIGINT NOT NULL,
    character_id BIGINT REFERENCES characters(id),
    alliance_id BIGINT REFERENCES alliances(id),
    kind SMALLINT CHECK (kind BETWEEN 1 AND 10),
    quantity INTEGER CHECK (quantity BETWEEN 1 AND 9999),
    coins BIGINT CHECK (coins > 0),
    -- Self-contained snapshot: frozen source rows may later be erased with their characters.
    details JSONB NOT NULL CHECK (jsonb_typeof(details) = 'object'),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    released_at TIMESTAMPTZ,
    UNIQUE (source, source_id),
    CHECK (num_nonnulls(character_id, alliance_id) = 1),
    CHECK ((kind IS NOT NULL AND quantity IS NOT NULL AND coins IS NULL)
        OR (kind IS NULL AND quantity IS NULL AND coins IS NOT NULL))
);
CREATE INDEX legacy_property_character ON legacy_property(character_id) WHERE released_at IS NULL;
CREATE INDEX legacy_property_alliance ON legacy_property(alliance_id) WHERE released_at IS NULL;

-- Escrowed attachments: neither claimed nor bounced. They belong to whoever originally sent
-- the item (for a bounced item that is the recipient of the return letter).
INSERT INTO legacy_property(source, source_id, character_id, kind, quantity, details)
SELECT 'mail_attachment', m.id,
    CASE WHEN m.return_of IS NULL THEN m.sender_id ELSE m.recipient_id END,
    m.attachment_kind, m.attachment_quantity,
    jsonb_build_object(
        'sender', m.sender_id,
        'recipient', m.recipient_id,
        'price', m.attachment_price,
        'token', m.attachment_token,
        'return_of', m.return_of,
        'subject', m.subject,
        'sent_at', m.sent_at)
FROM character_mail m
WHERE m.attachment_kind IS NOT NULL AND NOT m.attachment_claimed AND NOT m.attachment_returned;

-- Coins already paid for claimed attachments but not yet credited to the seller.
INSERT INTO legacy_property(source, source_id, character_id, coins, details)
SELECT 'mail_payment', p.mail_id, p.owner_id, p.amount, jsonb_build_object('mail', p.mail_id)
FROM mail_payments p
WHERE NOT p.settled;

INSERT INTO legacy_property(source, source_id, alliance_id, kind, quantity, details)
SELECT 'alliance_storage', s.id, s.alliance_id, s.kind, s.quantity, jsonb_build_object('slot', s.slot)
FROM alliance_storage s;

-- Exclusive profession choice and its experience are archived, then removed from active stats.
-- craft_count (the anti-replay counter of crafting) stays.
INSERT INTO legacy_character_archive(character_id, migration, data, converted_at)
SELECT id, '0017_remove_mail_storage_professions', jsonb_strip_nulls(jsonb_build_object(
    'craft_specialization', stats->'craft_specialization',
    'craft_specialization_revision', stats->'craft_specialization_revision',
    'craft_xp_leather', stats->'craft_xp_leather',
    'craft_xp_smithing', stats->'craft_xp_smithing',
    'craft_xp_runes', stats->'craft_xp_runes')), now()
FROM characters
WHERE stats ?| ARRAY['craft_specialization', 'craft_specialization_revision',
    'craft_xp_leather', 'craft_xp_smithing', 'craft_xp_runes'];

UPDATE characters SET stats = stats - 'craft_specialization' - 'craft_specialization_revision'
    - 'craft_xp_leather' - 'craft_xp_smithing' - 'craft_xp_runes'
WHERE stats ?| ARRAY['craft_specialization', 'craft_specialization_revision',
    'craft_xp_leather', 'craft_xp_smithing', 'craft_xp_runes'];
