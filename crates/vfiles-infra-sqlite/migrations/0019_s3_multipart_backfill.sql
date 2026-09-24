CREATE TABLE s3_multipart_backfill (
    namespace_id TEXT PRIMARY KEY REFERENCES namespaces(id) ON DELETE CASCADE
);
