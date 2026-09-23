-- Preserve existing lock expiry instants while moving to millisecond precision.
UPDATE webdav_locks
SET expires_at = expires_at * 1000
WHERE expires_at IS NOT NULL;
