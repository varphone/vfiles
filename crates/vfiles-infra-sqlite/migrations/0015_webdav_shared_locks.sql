CREATE TABLE webdav_locks_shared (
    namespace_id TEXT NOT NULL REFERENCES namespaces(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    token TEXT NOT NULL UNIQUE,
    owner TEXT NOT NULL,
    expires_at INTEGER,
    depth_infinity INTEGER NOT NULL DEFAULT 0 CHECK (depth_infinity IN (0, 1)),
    scope TEXT NOT NULL DEFAULT 'exclusive' CHECK (scope IN ('exclusive', 'shared')),
    PRIMARY KEY (namespace_id, path, token)
);

INSERT INTO webdav_locks_shared
    (namespace_id, path, token, owner, expires_at, depth_infinity, scope)
SELECT namespace_id, path, token, owner, expires_at, depth_infinity, 'exclusive'
FROM webdav_locks;

DROP TABLE webdav_locks;
ALTER TABLE webdav_locks_shared RENAME TO webdav_locks;

CREATE INDEX idx_webdav_locks_expires_at ON webdav_locks(expires_at);
