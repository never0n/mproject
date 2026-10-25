-- The inlet chosen when a pour starts. A retry after a crash uses this row, not a later command.
ALTER TABLE craft_sessions
    ADD COLUMN pour_seconds INTEGER NOT NULL DEFAULT 0
        CHECK (pour_seconds >= 0 AND pour_seconds <= 60);
