-- Directory history asks two questions per request: which snapshots contain a
-- path, and the newest of them. Both used to scan every entry of every
-- snapshot in the namespace (rows in `snapshots` order, then a temp sort),
-- which grows with the whole history instead of with the answer.
--
-- `idx_snapshots_namespace_created` lets the page walk snapshots newest first
-- and stop after `limit` matches; `idx_snapshot_entries_path_snapshot` lets
-- each existence check become an index seek instead of a scan of that
-- snapshot's entries.
CREATE INDEX IF NOT EXISTS idx_snapshots_namespace_created
    ON snapshots(namespace_id, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_snapshot_entries_path_snapshot
    ON snapshot_entries(entry_path, snapshot_id);
