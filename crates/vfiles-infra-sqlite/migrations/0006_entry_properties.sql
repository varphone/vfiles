-- r13 自定义属性 k/v（WebDAV PROPPATCH 可写集扩展 ✗ r6 属性持久化债还清）
-- entry 级键值：移动/改名 entry_id 不变 = 属性跟资源 ✓ FK 级联 = 删 entry 白捡清理
CREATE TABLE IF NOT EXISTS entry_properties (
    entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
    prop_name TEXT NOT NULL,
    prop_value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (entry_id, prop_name)
);
CREATE INDEX IF NOT EXISTS idx_entry_properties_entry ON entry_properties(entry_id);
