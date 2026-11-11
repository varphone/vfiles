-- Snapshot storage used to copy the full namespace tree for every mutation.
-- A 12.6k-entry workspace with 13.2k snapshots produced ~85M rows.
--
-- New snapshots are either a full checkpoint or a delta relative to the latest
-- checkpoint. Existing snapshots stay in `snapshot_entries` as `legacy` and
-- remain readable; this migration only adds metadata, a delta table and a
-- compact path-presence index, so it does not scan the 85M existing rows.

ALTER TABLE snapshots ADD COLUMN sequence INTEGER;
ALTER TABLE snapshots ADD COLUMN storage_kind TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE snapshots ADD COLUMN checkpoint_id TEXT;

-- Existing snapshots get a per-namespace order. New snapshots allocate
-- MAX(sequence)+1 in the same transaction that creates the snapshot.
WITH ranked AS (
    SELECT
        id,
        ROW_NUMBER() OVER (
            PARTITION BY namespace_id
            ORDER BY created_at ASC, id ASC
        ) AS sequence
    FROM snapshots
)
UPDATE snapshots
SET sequence = (SELECT ranked.sequence FROM ranked WHERE ranked.id = snapshots.id);

CREATE UNIQUE INDEX idx_snapshots_namespace_sequence
    ON snapshots(namespace_id, sequence);

CREATE INDEX idx_snapshots_namespace_checkpoint
    ON snapshots(namespace_id, checkpoint_id);

-- Delta rows are stored separately from `snapshot_entries` because a rename
-- needs both an old-path tombstone and a new-path row for the same entry; the
-- old table's primary key is (snapshot_id, entry_id).
CREATE TABLE snapshot_entry_changes (
    snapshot_id TEXT NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    entry_id TEXT NOT NULL,
    entry_path TEXT NOT NULL,
    parent_path TEXT GENERATED ALWAYS AS (
        rtrim(rtrim(entry_path, replace(entry_path, '/', '')), '/')
    ) VIRTUAL,
    entry_kind TEXT NOT NULL CHECK (entry_kind IN ('file', 'directory')),
    entry_version_id TEXT,
    blob_id TEXT,
    size INTEGER,
    content_type TEXT,
    version_no INTEGER,
    change_type TEXT NOT NULL CHECK (change_type IN ('added', 'modified', 'deleted', 'renamed')),
    created_by TEXT,
    created_at TEXT,
    PRIMARY KEY (snapshot_id, entry_id, entry_path)
);

CREATE INDEX idx_snapshot_entry_changes_snapshot_parent
    ON snapshot_entry_changes(snapshot_id, parent_path, entry_path);

CREATE INDEX idx_snapshot_entry_changes_path_snapshot
    ON snapshot_entry_changes(entry_path, snapshot_id);

-- Path presence intervals preserve the existing directory-history semantics
-- (all snapshots that contain a path) without one row per path per snapshot.
CREATE TABLE snapshot_path_ranges (
    namespace_id TEXT NOT NULL REFERENCES namespaces(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    entry_id TEXT NOT NULL,
    entry_kind TEXT NOT NULL CHECK (entry_kind IN ('file', 'directory')),
    start_seq INTEGER NOT NULL,
    end_seq INTEGER,
    PRIMARY KEY (namespace_id, path, start_seq, entry_id)
);

CREATE INDEX idx_snapshot_path_ranges_path_end
    ON snapshot_path_ranges(namespace_id, path, end_seq);

-- All entries that are live at upgrade time are present in every new
-- snapshot until they are renamed or deleted. Seed open intervals at the
-- first sequence that a new snapshot can use.
INSERT INTO snapshot_path_ranges (
    namespace_id, path, entry_id, entry_kind, start_seq, end_seq
)
SELECT
    e.namespace_id,
    e.path,
    e.id,
    e.kind,
    COALESCE(
        (SELECT MAX(s.sequence) + 1 FROM snapshots s WHERE s.namespace_id = e.namespace_id),
        1
    ),
    NULL
FROM entries e;
