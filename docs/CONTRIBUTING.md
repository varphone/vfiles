# 贡献指南

本项目当前以 Rust 后端 + Vue 前端为主，提交前请先确认变更落在当前实现路径上，不要再把旧 TypeScript 服务端相关内容加回来。

## 开发环境

- Rust stable
- Bun
- Git

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

## 提交前建议

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
