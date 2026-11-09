-- Live directory listings located direct children with `LIKE 'prefix/%'` plus
-- an `instr(...)` check over the whole subtree: a page of a large directory
-- cost one scan of every descendant, twice (count + page). The derived
-- `parent_path` turns the same lookup into an index seek, and the index order
-- matches the "directories first, then path" listing order.
ALTER TABLE entries
    ADD COLUMN parent_path TEXT GENERATED ALWAYS AS (
        rtrim(rtrim(path, replace(path, '/', '')), '/')
    ) VIRTUAL;

CREATE INDEX idx_entries_namespace_parent_kind_path
    ON entries(namespace_id, parent_path, kind, path);
