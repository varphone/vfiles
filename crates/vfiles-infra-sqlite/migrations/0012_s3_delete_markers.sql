CREATE TABLE s3_delete_markers (
    version_id TEXT PRIMARY KEY NOT NULL,
    namespace_id TEXT NOT NULL REFERENCES namespaces(id) ON DELETE CASCADE,
    object_key TEXT NOT NULL,
    owner_id TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL
);

CREATE INDEX idx_s3_delete_markers_namespace_key_created
    ON s3_delete_markers(namespace_id, object_key, created_at DESC, version_id DESC);
