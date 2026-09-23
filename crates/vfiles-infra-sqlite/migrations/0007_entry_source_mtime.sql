-- r18 rsync 默认快跳（size+mtime）：记录**源端文件 mtime**（秒 ✗ NULL = 未知/非 rsync 来源）
-- 只加列不改语义：既有行 NULL → rsync 端退化为「尺寸相同也不跳」（安全侧）
ALTER TABLE entry_versions ADD COLUMN source_mtime INTEGER;
