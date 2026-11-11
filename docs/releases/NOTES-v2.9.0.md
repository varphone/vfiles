## VFiles 2.9.0（未发布）

快照存储重构版本。目标是把“每次写操作复制整棵目录树”改为增量快照，避免频繁上传/删除时
数据库按 `条目数 × 快照数` 膨胀。

### 快照存储

- 自动快照改为 **checkpoint + delta**：普通变更只写 `snapshot_entry_changes`，每
  `VFILES_SNAPSHOT_CHECKPOINT_INTERVAL`（默认 1024）个快照或手动快照写一次完整 checkpoint。
- 旧版全量快照保持 `legacy` 可读；升级迁移 `0035` 不扫描、不重写已有的
  `snapshot_entries` 行。
- 新增 `snapshot_path_ranges` 路径存在区间，目录历史仍返回“路径存在的所有快照”，
  不需要每个快照保存一份完整路径。
- 裁剪按完整 checkpoint epoch 删除，避免留下依赖已删除 checkpoint 的 delta。
- 裁剪按批（每批最多 64 个快照）聚合 blob 引用并提交删除，降低长事务和 WAL 峰值；
  孤儿 blob 清理改用 `blobs.ref_count`，不再扫描整张历史快照表。
- 新增离线 `rebuild-snapshots`：只复制保留快照的条目到新表，替代对数千万条 legacy
  数据的逐行删除；需要停机、备份和额外磁盘空间，可选重建后 `VACUUM`。
- 废弃 `VFILES_SNAPSHOT_COALESCE_SECONDS`：增量快照已替代旧合并窗口；设置后启动仅告警。

### 升级说明

- 新增 SQLite 迁移 `0035`；大库升级不再重复建历史索引，但仍会为当前条目建立路径区间。
- 旧快照不会被删除或改写；如需缩小现有 70GB 级数据库，仍需按保留策略裁剪旧快照并
  `VACUUM`（或执行后续提供的离线压缩工具）。
- 一旦写入新格式快照，不能直接降级到 2.8.x；回退需要恢复升级前备份。
- 目录历史、快照浏览、下载和恢复 API 保持兼容。

### 升级后建议

```bash
VFILES_SNAPSHOT_CHECKPOINT_INTERVAL=1024
# 按需启用保留策略；先备份，再执行
# VFILES_MAINTENANCE_ENABLED=true
# VFILES_MAINTENANCE_SNAPSHOT_KEEP=1000
```
