# 贡献指南

本项目当前以 Rust 后端 + Vue 前端为主，提交前请先确认变更落在当前实现路径上，不要再把旧 TypeScript 服务端相关内容加回来。

## 开发环境

- Rust stable
- Bun
- Git

## 本地数据与测试夹具

CLI（`vfiles import` 等）与服务器一样按 `VFILES_STORAGE_ROOT` / `VFILES_DATABASE_PATH`
等环境变量定位存储；**未导出时会写入仓库默认的 `data/`（真实数据）**。造夹具/跑临时
服务器时，请在**同一个 shell** 中先导出与该服务器一致的环境变量再执行任何 CLI。
（2026-09 有一次漏导环境变量把夹具写进了真实库的事故，已清理；见
OPTIMIZATION_ROADMAP §4.24。）

## 依赖安装

```bash
cd client && bun install
```

## 常用命令

```bash
# 后端开发
cargo run --package vfiles-bin --bin vfiles -- serve

# 前端开发
cd client && bun run dev

# Rust 测试
cargo test --workspace

# 前端单测
cd client && bun run test

# 构建前端
cd client && bun run build

# Lint / 格式化
cd client && bun run lint
cd client && bun run fmt
```

## 前端样式与主题

- 颜色一律使用设计令牌，不要在组件里写死色值：令牌定义在
  `client/src/styles/theme.scss`（`--vf-*`），浅色与深色各一套。
- 结构性颜色优先复用 Bulma 变量（`--bulma-*`）；Bulma 主题由
  `client/src/styles/bulma.scss` 引入，跟随 `<html data-theme="light|dark">`
  或系统 `prefers-color-scheme`。
- 新增令牌时同时补浅色与深色两组值，并确保真的有组件在用。
- 主题切换逻辑在 `client/src/stores/theme.store.ts`，界面入口是
  `client/src/components/common/ThemeToggle.vue`。

## 提交前建议

- `bun run lint:styles`：死样式扫描（零引用类、无动画引用的 keyframes、花括号不配平），
  对 transition 运行时类、`` `...${...}` `` 拼接前缀族与 highlight.js 类已内置豁免。

1. 只提交和当前任务直接相关的改动。
2. 如果改了 `client/package.json`，同步刷新 `client/bun.lockb`。
3. 如果改了 Rust 测试辅助代码，至少补一轮 `cargo test --no-run` 或对应测试。
4. 如果改了前端组件，优先补最窄的 Vitest 回归。
5. 文档统一放在 `docs/`；根目录除 README 外不再堆放说明文档。

## 分支与提交信息

建议使用 Conventional Commits：

- `feat:` 新功能
- `fix:` 修复
- `docs:` 文档调整
- `refactor:` 重构
- `chore:` 工具链或仓库整理

## Pull Request 说明

请在 PR 中至少写清：

- 改了什么
- 为什么要改
- 怎么验证
- 还有哪些已知限制或未覆盖点
