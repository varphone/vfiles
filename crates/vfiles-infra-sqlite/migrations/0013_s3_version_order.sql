CREATE TABLE s3_version_sequence (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    value INTEGER NOT NULL
);

INSERT INTO s3_version_sequence (id, value) VALUES (1, 0);

ALTER TABLE entry_versions ADD COLUMN created_order INTEGER NOT NULL DEFAULT 0;
ALTER TABLE s3_delete_markers ADD COLUMN event_order INTEGER NOT NULL DEFAULT 0;

CREATE INDEX idx_entry_versions_entry_created_order
    ON entry_versions(entry_id, created_order DESC);
CREATE INDEX idx_s3_delete_markers_namespace_key_event_order
    ON s3_delete_markers(namespace_id, object_key, event_order DESC);
