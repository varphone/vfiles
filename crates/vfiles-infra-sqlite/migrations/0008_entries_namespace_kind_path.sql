-- Keep live root directory pages in their required directory-first/name order
-- without sorting the matching entries into a temporary B-tree on each request.
CREATE INDEX idx_entries_namespace_kind_path
    ON entries(namespace_id, (kind = 'directory') DESC, path);
