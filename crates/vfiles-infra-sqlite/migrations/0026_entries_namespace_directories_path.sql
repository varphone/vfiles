-- Directory picker COUNT/page queries filter by namespace and kind. Keeping
-- only directory rows makes the index covering for COUNT without duplicating
-- every file entry in another index.
CREATE INDEX idx_entries_namespace_directories_path
ON entries(namespace_id, path)
WHERE kind = 'directory';
