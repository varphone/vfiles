UPDATE s3_multipart_uploads
SET initiated_at = initiated_at * 1000000;

DROP INDEX idx_s3_multipart_uploads_listing;

CREATE INDEX idx_s3_multipart_uploads_listing
    ON s3_multipart_uploads(namespace_id, object_key, initiated_at, upload_id);
