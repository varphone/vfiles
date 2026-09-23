CREATE TABLE webdav_locks (
    namespace_id TEXT NOT NULL REFERENCES namespaces(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    token TEXT NOT NULL UNIQUE,
    owner TEXT NOT NULL,
    expires_at INTEGER,
    PRIMARY KEY (namespace_id, path)
);

CREATE INDEX idx_webdav_locks_expires_at ON webdav_locks(expires_at);
