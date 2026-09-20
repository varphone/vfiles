-- Indexes for authentication and blob-reference hot paths.

CREATE UNIQUE INDEX IF NOT EXISTS idx_user_sessions_token_hash
    ON user_sessions(session_token_hash);

CREATE INDEX IF NOT EXISTS idx_entry_versions_blob_id
    ON entry_versions(blob_id);

CREATE INDEX IF NOT EXISTS idx_snapshot_entries_entry_id
    ON snapshot_entries(entry_id);
