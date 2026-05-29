-- Consolidated initial schema for VFiles.

CREATE TABLE users (
    id TEXT PRIMARY KEY NOT NULL,
    username TEXT NOT NULL UNIQUE,
    email TEXT UNIQUE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('admin', 'manager', 'user')),
    disabled BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    password_changed_at TEXT
);

CREATE TABLE user_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    session_token_hash TEXT NOT NULL,
    issued_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL,
    revoked_at TEXT,
    user_agent TEXT,
    ip_addr TEXT,
    last_seen_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE namespaces (
    id TEXT PRIMARY KEY NOT NULL,
    slug TEXT NOT NULL,
    owner_user_id TEXT NOT NULL REFERENCES users(id),
    kind TEXT NOT NULL DEFAULT 'user',
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(owner_user_id, slug)
);

CREATE TABLE system_settings (
    key TEXT PRIMARY KEY NOT NULL,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE blobs (
    id TEXT PRIMARY KEY NOT NULL,
    content_hash TEXT NOT NULL UNIQUE,
    storage_key TEXT NOT NULL,
    size INTEGER NOT NULL,
    content_type TEXT,
    ref_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    uploaded_by TEXT REFERENCES users(id),
    verified_at TEXT
);

CREATE TABLE entries (
    id TEXT PRIMARY KEY NOT NULL,
    namespace_id TEXT NOT NULL REFERENCES namespaces(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('file', 'directory')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(namespace_id, path)
);

CREATE TABLE entry_versions (
    id TEXT PRIMARY KEY NOT NULL,
    entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    blob_id TEXT REFERENCES blobs(id),
    size INTEGER,
    content_type TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT NOT NULL REFERENCES users(id),
    message TEXT,
    UNIQUE(entry_id, version)
);

CREATE TABLE snapshots (
    id TEXT PRIMARY KEY NOT NULL,
    namespace_id TEXT NOT NULL REFERENCES namespaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT NOT NULL REFERENCES users(id),
    UNIQUE(namespace_id, name)
);

CREATE TABLE snapshot_entries (
    snapshot_id TEXT NOT NULL,
    entry_id TEXT NOT NULL,
    entry_version_id TEXT,
    entry_path TEXT NOT NULL,
    entry_kind TEXT NOT NULL CHECK (entry_kind IN ('file', 'directory')),
    blob_id TEXT,
    size INTEGER,
    content_type TEXT,
    version_no INTEGER,
    change_type TEXT NOT NULL CHECK (change_type IN ('added', 'modified', 'deleted', 'renamed')),
    created_by TEXT,
    created_at TEXT,
    PRIMARY KEY (snapshot_id, entry_id),
    FOREIGN KEY (snapshot_id) REFERENCES snapshots(id) ON DELETE CASCADE
);

CREATE TABLE upload_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    namespace_id TEXT NOT NULL REFERENCES namespaces(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    total_size INTEGER NOT NULL,
    content_type TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'completed', 'expired', 'cancelled'))
);

CREATE TABLE upload_parts (
    id TEXT PRIMARY KEY NOT NULL,
    upload_session_id TEXT NOT NULL REFERENCES upload_sessions(id) ON DELETE CASCADE,
    part_number INTEGER NOT NULL,
    size INTEGER NOT NULL,
    offset INTEGER NOT NULL,
    blob_id TEXT REFERENCES blobs(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(upload_session_id, part_number)
);

CREATE TABLE shares (
    id TEXT PRIMARY KEY NOT NULL,
    namespace_id TEXT NOT NULL REFERENCES namespaces(id) ON DELETE CASCADE,
    entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
    entry_version_id TEXT REFERENCES entry_versions(id),
    code TEXT NOT NULL UNIQUE,
    expires_at TEXT,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    access_count INTEGER NOT NULL DEFAULT 0,
    last_accessed_at TEXT,
    disabled_at TEXT
);

-- Search reads blob content from BlobStore directly, so the FTS table exists
-- without database triggers that assume blobs.content is stored in SQLite.
CREATE VIRTUAL TABLE content_fts USING fts5(
    entry_version_id UNINDEXED,
    content,
    content='',
    content_rowid='rowid'
);

CREATE INDEX idx_user_sessions_user_id ON user_sessions(user_id);
CREATE INDEX idx_user_sessions_expires_at ON user_sessions(expires_at);
CREATE INDEX idx_namespaces_owner_user_id ON namespaces(owner_user_id);
CREATE INDEX idx_entries_namespace_path ON entries(namespace_id, path);
CREATE INDEX idx_entry_versions_entry_id ON entry_versions(entry_id);
CREATE INDEX idx_snapshots_namespace_name ON snapshots(namespace_id, name);
CREATE INDEX idx_snapshot_entries_snapshot_id ON snapshot_entries(snapshot_id);
CREATE INDEX idx_snapshot_entries_snapshot_path ON snapshot_entries(snapshot_id, entry_path);
CREATE INDEX idx_upload_sessions_namespace_path ON upload_sessions(namespace_id, path);
CREATE INDEX idx_upload_parts_session_id ON upload_parts(upload_session_id);
CREATE INDEX idx_shares_created_by ON shares(created_by);
CREATE INDEX idx_shares_entry_id ON shares(entry_id);
CREATE INDEX idx_shares_expires_at ON shares(expires_at);
CREATE INDEX idx_blobs_created_at ON blobs(created_at);