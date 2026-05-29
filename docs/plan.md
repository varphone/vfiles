# VFiles Rust 重写实施计划

## 项目概述

VFiles 是一个基于 Git 的文件管理系统，使用 Rust 重写以提高性能和安全性。

## 当前进度

### 已完成阶段

#### PR4: HTTP 基础 ✅
- [x] Axum HTTP 服务器设置
- [x] 基础路由结构
- [x] 中间件配置（CORS、日志、限流）
- [x] 健康检查端点
- [x] 错误处理
- [x] 配置管理

#### PR5: 认证基础 ✅
- [x] 用户领域模型 (User, UserSession, AuthUser)
- [x] 认证服务 (AuthService)
- [x] 会话管理 (SessionService)
- [x] 用户仓库 (SqliteUserRepo)
- [x] 会话仓库 (SqliteSessionRepo)
- [x] HTTP 认证路由 (login, register, logout, me)
- [x] Cookie 基础会话管理
- [x] 密码哈希和验证

### 进行中阶段

#### PR6: 文件管理基础
- [ ] 文件领域模型
- [ ] 文件服务
- [ ] Git 集成服务
- [ ] 文件仓库
- [ ] 文件 HTTP 路由

#### PR7: 高级功能
- [ ] 版本历史
- [ ] 文件搜索
- [ ] 批量操作
- [ ] 文件预览

#### PR8: 前端集成
- [ ] 更新前端以使用新的 Rust API
- [ ] 认证集成
- [ ] 文件管理集成

### 计划阶段

#### Phase 1: 核心功能完成
- [ ] 文件上传/下载
- [ ] 目录浏览
- [ ] Git 历史查看
- [ ] 用户权限管理

#### Phase 2: 高级功能
- [ ] 文件搜索
- [ ] 版本对比
- [ ] 批量操作
- [ ] 文件预览

#### Phase 3: 性能优化
- [ ] 缓存机制
- [ ] 并发处理
- [ ] 大文件处理
- [ ] 数据库优化

#### Phase 4: 测试和部署
- [ ] 单元测试
- [ ] 集成测试
- [ ] E2E 测试
- [ ] Docker 部署
- [ ] CI/CD 配置

## 技术栈

- **语言**: Rust 2021 Edition
- **Web 框架**: Axum
- **数据库**: SQLite with SQLx
- **认证**: Cookie-based sessions
- **密码哈希**: SHA2
- **前端**: Vue 3 + TypeScript (保持不变)

## 架构

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   HTTP Layer    │    │   App Layer     │    │  Domain Layer   │
│   (vfiles-http) │◄──►│  (vfiles-app)   │◄──►│ (vfiles-domain) │
│                 │    │                 │    │                 │
│ - Routes        │    │ - Services      │    │ - Entities      │
│ - DTOs          │    │ - Business Logic│    │ - Value Objects │
│ - Middleware    │    │                 │    │ - Domain Errors │
└─────────────────┘    └─────────────────┘    └─────────────────┘
                                                         ▲
                                                         │
┌─────────────────┐    ┌─────────────────┐               │
│ Infra Layer     │    │ Infra Layer     │               │
│ (vfiles-infra-  │    │ (vfiles-infra-  │               │
│    sqlite)      │    │     fs)         │               │
│                 │    │                 │               │
│ - Repositories  │    │ - File System   │               │
│ - Migrations    │    │ - Git Commands  │               │
└─────────────────┘    └─────────────────┘               │
                                                         │
┌─────────────────┐                                       │
│   Config        │                                       │
│ (vfiles-config) │◄──────────────────────────────────────┘
│                 │
│ - App Config    │
│ - Environment   │
└─────────────────┘
```

## 下一步计划

1. **立即**: 实现文件管理基础 (PR6)
2. **短期**: 完成核心文件操作功能
3. **中期**: 集成前端和高级功能
4. **长期**: 性能优化和生产部署

## 风险和挑战

1. **Git 集成复杂度**: 需要仔细处理 Git 命令和错误情况
2. **大文件处理**: 需要实现分片上传和流式下载
3. **并发控制**: 多用户同时操作时的冲突处理
4. **性能优化**: Git 操作在大仓库下的性能问题

## 验收标准

- [ ] 所有核心功能正常工作
- [ ] 通过基本测试套件
- [ ] 可以部署到生产环境
- [ ] 性能满足基本要求