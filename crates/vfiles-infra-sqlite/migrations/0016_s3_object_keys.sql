-- Keep the flat S3 key namespace separate from filesystem path spelling.
-- Existing S3-compatible objects use their filesystem path as their key, so
-- seed those mappings before the S3 adapter starts consulting this index.
CREATE TABLE s3_object_keys (
    namespace_id TEXT NOT NULL REFERENCES namespaces(id) ON DELETE CASCADE,
    object_key TEXT NOT NULL,
    entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
    PRIMARY KEY (namespace_id, object_key),
    UNIQUE (namespace_id, entry_id)
);

CREATE INDEX idx_s3_object_keys_entry
    ON s3_object_keys(namespace_id, entry_id);

INSERT INTO s3_object_keys (namespace_id, object_key, entry_id)
SELECT namespace_id, path, id
FROM entries
WHERE kind = 'file';
