-- Root listing totals previously scanned every entry in the namespace. This
-- covering partial index contains only directly addressable root entries.
CREATE INDEX idx_entries_namespace_root_path
ON entries(namespace_id, path)
WHERE instr(path, '/') = 0;
