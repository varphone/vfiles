-- Browsing a directory inside a snapshot used to derive its direct children
-- by scanning every descendant of the requested path and grouping the first
-- path segment. That cost grew with the size of the whole tree for one page of
-- results; a 50k-entry snapshot needed two such passes (count + page) per
-- request.
--
-- `parent_path` is derived from `entry_path` so the same lookup becomes a
-- range seek over one directory's children. Snapshots written by this service
-- always record every directory as well as its files, so a child never needs
-- to be synthesized from a deeper path.
ALTER TABLE snapshot_entries
    ADD COLUMN parent_path TEXT GENERATED ALWAYS AS (
        rtrim(rtrim(entry_path, replace(entry_path, '/', '')), '/')
    ) VIRTUAL;

-- `change_type` is part of the index so counting and paging children can
-- filter deleted rows without touching the table.
CREATE INDEX idx_snapshot_entries_snapshot_parent
    ON snapshot_entries(snapshot_id, parent_path, entry_path, change_type);

-- The sidebar's "recent files" list ordered every file by its latest version's
-- timestamp through a correlated subquery. `entries.updated_at` already tracks
-- that moment, so a partial ordering index makes the list a short index scan.
CREATE INDEX idx_entries_recent_updated
    ON entries(namespace_id, updated_at DESC)
    WHERE kind = 'file';
