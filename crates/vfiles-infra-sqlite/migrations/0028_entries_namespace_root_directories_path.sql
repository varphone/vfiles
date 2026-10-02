-- The root directory picker counts only direct child directories. Keep those
-- rows in a dedicated covering index instead of scanning every directory.
CREATE INDEX idx_entries_namespace_root_directories_path
ON entries(namespace_id, path)
WHERE kind = 'directory' AND instr(path, '/') = 0;
