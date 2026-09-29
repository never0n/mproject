ALTER TABLE chat_messages DROP CONSTRAINT chat_messages_channel_check;
ALTER TABLE chat_messages ADD CONSTRAINT chat_messages_channel_check CHECK(channel IN ('general','local','group'));
