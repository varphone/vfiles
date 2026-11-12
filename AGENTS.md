# AGENTS.md

本文件是仓库级的协作约定，优先级高于“每次改动都跑完整测试”的默认习惯。

## 测试与 CI 策略

### 日常开发

日常开发、重构、修 bug 时，**不要求运行完整 CI 测试套件**，也不要求等待 GitHub Actions 结果。

按改动范围做针对性的最小验证即可，例如：

- Rust：`cargo check -p <crate>`，以及受影响 crate 的单个测试；
- 前端：`bun run typecheck`、`bun run lint`，以及受影响的 `vitest` 文件；
- 样式/模板：只验证实际改动的页面或组件，不跑完整视觉回归。

日常提交不因为 GitHub CI 失败而阻塞，也不需要为了普通提交等待约 15 分钟的 CI；后续发布前统一处理。

### 发布新版本前

只有准备发布新版本时，才执行完整验证。发布前必须完成以下项目，任一失败都不得打 tag 或发布：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --exclude vfiles-ftp
cargo test -p vfiles-ftp -- --test-threads=1
cargo test -p vfiles-webdav --features e2e
cargo audit
cargo deny check

cd client
bun run check
bun run size:check
bun run ui:regression
bun run ui:visual
```

此外，发布前还应：

- 运行必要的协议探针（WebDAV、S3、rsync）；
- 更新版本号、CHANGELOG、发布说明和受版本号影响的视觉基线；
- 构建并核对发布附件与 SHA-256；
- 打 tag 后确认该 tag 的 GitHub Actions CI 为绿色；CI 失败时不得把该版本报告为已验证，需修复后发布补丁版本或重新发布。

### 例外情况

以下改动即使不在发布前，也应扩大验证范围：

- 数据库迁移、快照存储、权限/安全边界；
- 公共 API、协议兼容性或并发关键路径；
- CI 工作流、测试基础设施、像素基线；
- 用户明确要求运行测试或 CI。

## 沟通要求

- 不要声称“CI 已通过”或“全量测试通过”，除非确实运行并看到结果。
- 日常改动只需报告实际执行的针对性检查；完整验证结果只在发布流程中报告。
