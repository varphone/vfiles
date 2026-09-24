CREATE TRIGGER entries_s3_object_key_insert
AFTER INSERT ON entries
WHEN NEW.kind = 'file'
BEGIN
    INSERT OR IGNORE INTO s3_object_keys (namespace_id, object_key, entry_id)
    VALUES (NEW.namespace_id, NEW.path, NEW.id);
END;

CREATE TRIGGER entries_s3_object_key_path_update
AFTER UPDATE OF namespace_id, path ON entries
WHEN NEW.kind = 'file'
BEGIN
    UPDATE s3_object_keys
    SET namespace_id = NEW.namespace_id, object_key = NEW.path
    WHERE entry_id = NEW.id
      AND object_key = OLD.path
      AND NOT EXISTS (
          SELECT 1 FROM s3_object_keys existing
          WHERE existing.namespace_id = NEW.namespace_id
            AND existing.object_key = NEW.path
            AND existing.entry_id <> NEW.id
      );
END;
