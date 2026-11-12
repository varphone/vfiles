## VFiles 2.9.1（2026-11-12）

CI/测试修复版本。运行时行为与 v2.9.0 相同，没有数据库迁移或 API 变化。

### 修复内容

- CI/Release 工作流安装 `sccache`，修复 `.cargo/config.toml` 中的
  `rustc-wrapper = "sccache"` 在 GitHub runner 上找不到二进制的问题。
- 安装 `rustfmt` 和 `clippy` 组件，使 `cargo fmt --check` 与 clippy 检查可运行。
- 修复 coverage 下 `FileBrowser` 预览复制按钮偶尔超过 1 秒等待时间的单测。
- 将 runner 固定到 `ubuntu-24.04`；新增 GitHub Actions 专用像素基线
  `client/tests/visual/goldens-ci/`，避免本地与 CI 的字体抗锯齿差异造成全量视觉失败。
- 视觉回归失败时上传 `visual-diffs` artifact，便于排查。

### 升级说明

- 无数据库迁移；迁移 `0035` 已由 2.9.0 引入，2.9.0 数据库可直接运行 2.9.1。
- 运行时行为的差异仅来自版本号；升级主要用于获得通过的发布 tag 和 CI 记录。
- 如需使用 2.9.0 的增量快照和大库重建能力，建议升级到 2.9.1。
