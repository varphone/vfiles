-- Same account, several passwords: the primary password stays in
-- `users.password_hash`, scoped credentials live here. A session records which
-- credential signed it in; NULL means the primary password (full scope).
CREATE TABLE user_credentials (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    -- JSON array of normalized directory roots, e.g. ["photos","docs/2024"].
    scope_roots TEXT NOT NULL,
    can_write BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_used_at TEXT
);

CREATE INDEX idx_user_credentials_user ON user_credentials(user_id);

ALTER TABLE user_sessions ADD COLUMN credential_id TEXT REFERENCES user_credentials(id) ON DELETE CASCADE;

CREATE INDEX idx_user_sessions_credential ON user_sessions(credential_id);
