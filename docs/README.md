# 文档索引

当前仓库的说明文档已经从根目录收拢到 `docs/`。根目录只保留项目入口 README，其余指南、参考和历史计划统一在这里查找。

## 当前文档

- [QUICK_START.md](QUICK_START.md) - 本地跑通服务的最短路径
- [DEPLOYMENT.md](DEPLOYMENT.md) - 环境变量、发布目录、systemd 与反向代理建议
- [ADMIN_SETUP.md](ADMIN_SETUP.md) - 初始化管理员与常用 user 子命令
- [USER_GUIDE.md](USER_GUIDE.md) - 面向使用者的常见操作说明
- [API.md](API.md) - 当前 Rust HTTP API 概览
- [ARCHITECTURE.md](ARCHITECTURE.md) - 当前代码结构与运行时架构
- [CONTRIBUTING.md](CONTRIBUTING.md) - 本地开发、测试与提交流程
- [OPTIMIZATION_ROADMAP.md](OPTIMIZATION_ROADMAP.md) - 设计/实现审计与稳定性、性能、交互优化路线图

## 历史/规划资料

这些文档保留为项目演进记录，内容可能包含旧实现或已废弃方案，不应再作为当前操作基线：

- [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md)
- [RUST_IMPLEMENTATION_PLAN.md](RUST_IMPLEMENTATION_PLAN.md)
- [PERFORMANCE_OPTIMIZATION.md](PERFORMANCE_OPTIMIZATION.md)
- [plan.md](plan.md)

## 说明

- 仓库内已删除旧的 Docker / Docker Compose / Nginx 部署配置。
- 当前推荐部署方式是原生 Rust 二进制，前端通过外部 `client/dist` 或 `embed` feature 提供。
- 环境变量示例统一使用根目录的 [.env.example](../.env.example)。
