-- 收藏：按条目 ID 引用（而非路径），这样重命名/移动后收藏依然有效，
-- 条目被删除时收藏随外键级联清除。
CREATE TABLE favorites (
    namespace_id TEXT NOT NULL REFERENCES namespaces(id) ON DELETE CASCADE,
    entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (namespace_id, entry_id)
);

CREATE INDEX IF NOT EXISTS idx_favorites_namespace ON favorites(namespace_id, created_at DESC);
