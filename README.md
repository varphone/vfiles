# VFiles

VFiles 是一个基于 Rust、Axum、SQLite 和 Vue 3 的文件管理系统，提供目录浏览、上传下载、版本历史、分享链接和基础用户管理能力。

当前仓库已经移除了旧的 TypeScript 服务端以及 Docker/Compose 部署配置，默认维护方式是：前端静态资源 + Rust 原生服务二进制。

## 核心特性

- Rust + Axum 后端，使用结构化日志与 SQLite 持久化
- Vue 3 + TypeScript + Bulma 前端
- 文件与目录的上传、下载、移动、重命名、删除
- 历史版本、快照、分享链接、搜索
- 管理员/用户会话体系与可选多用户命名空间
- 可选 FTP(S) 批量导入（客户端递归上传目录，默认关闭，详见部署文档）

## 快速开始

1. 复制环境变量模板：

```bash
cp .env.example .env
```

2. 构建前端：

```bash
cd client && bun install && bun run build
cd ..
```

3. 构建 Rust 服务：

```bash
cargo build -p vfiles-bin --release
```

4. 初始化数据目录和数据库：

```bash
./target/release/vfiles init
```

5. 创建第一个管理员：

```bash
./target/release/vfiles user create \
  --username admin \
  --email admin@example.com \
  --role admin \
  --password change-me
```

6. 启动服务：

```bash
RUST_LOG=info ./target/release/vfiles serve
```

默认访问地址是 http://localhost:3000 。

## 开发

后端开发：

```bash
cargo run --package vfiles-bin --bin vfiles -- serve
```

前端开发：

```bash
cd client && bun run dev
```

## 测试与构建

```bash
# Rust 测试
cargo test --workspace

# 前端单测
cd client && bun run test

# 前端构建
cd client && bun run build

# Rust release 构建
cargo build -p vfiles-bin --release
```

## 文档

文档已整理到 docs 目录：

- [docs/README.md](docs/README.md) - 文档索引
- [docs/QUICK_START.md](docs/QUICK_START.md) - 快速上手
- [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) - 本地/服务器部署
- [docs/ADMIN_SETUP.md](docs/ADMIN_SETUP.md) - 管理员初始化
- [docs/API.md](docs/API.md) - HTTP API 概览
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) - 当前系统架构
- [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) - 开发与提交流程
- [docs/USER_GUIDE.md](docs/USER_GUIDE.md) - 常用操作说明

## 环境变量

示例配置见 [.env.example](.env.example)。

## 许可

MIT License
