ALTER TABLE webdav_locks
ADD COLUMN depth_infinity INTEGER NOT NULL DEFAULT 0 CHECK (depth_infinity IN (0, 1));
