CREATE TABLE s3_multipart_uploads (
    namespace_id TEXT NOT NULL REFERENCES namespaces(id) ON DELETE CASCADE,
    object_key TEXT NOT NULL,
    upload_id TEXT NOT NULL,
    initiated_at INTEGER NOT NULL,
    PRIMARY KEY (namespace_id, upload_id)
);

CREATE INDEX idx_s3_multipart_uploads_listing
    ON s3_multipart_uploads(namespace_id, object_key, upload_id);
