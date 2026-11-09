-- The sidebar overview needed the size and MIME type of every entry's latest
-- version, which cost one index lookup per file on every page load (and after
-- every mutation, because the sidebar refreshes). Keep the current version's
-- payload on the entry instead, maintained by triggers so raw-SQL writers are
-- covered as well; the latest version by number stays the source of truth.
ALTER TABLE entries ADD COLUMN current_size INTEGER;
ALTER TABLE entries ADD COLUMN current_content_type TEXT;

UPDATE entries SET
    current_size = (
        SELECT ev.size FROM entry_versions ev
        WHERE ev.entry_id = entries.id
        ORDER BY ev.version DESC
        LIMIT 1
    ),
    current_content_type = (
        SELECT ev.content_type FROM entry_versions ev
        WHERE ev.entry_id = entries.id
        ORDER BY ev.version DESC
        LIMIT 1
    );

CREATE TRIGGER entry_versions_current_after_insert
AFTER INSERT ON entry_versions
WHEN NEW.version = (SELECT MAX(version) FROM entry_versions WHERE entry_id = NEW.entry_id)
BEGIN
    UPDATE entries
    SET current_size = NEW.size, current_content_type = NEW.content_type
    WHERE id = NEW.entry_id;
END;

-- Only a delete that removes the newest version changes the current payload;
-- an entry being deleted takes its versions with it, so those deletes are skipped.
CREATE TRIGGER entry_versions_current_after_delete
AFTER DELETE ON entry_versions
WHEN EXISTS (SELECT 1 FROM entries e WHERE e.id = OLD.entry_id)
 AND OLD.version > COALESCE((SELECT MAX(version) FROM entry_versions WHERE entry_id = OLD.entry_id), 0)
BEGIN
    UPDATE entries
    SET current_size = (
            SELECT ev.size FROM entry_versions ev
            WHERE ev.entry_id = OLD.entry_id
            ORDER BY ev.version DESC
            LIMIT 1
        ),
        current_content_type = (
            SELECT ev.content_type FROM entry_versions ev
            WHERE ev.entry_id = OLD.entry_id
            ORDER BY ev.version DESC
            LIMIT 1
        )
    WHERE id = OLD.entry_id;
END;
