-- Every lookup by snapshot id is served by the primary key
-- `(snapshot_id, entry_id)`, so this secondary index only added disk and write
-- cost to the full-tree copy made for each mutation.
DROP INDEX IF EXISTS idx_snapshot_entries_snapshot_id;
