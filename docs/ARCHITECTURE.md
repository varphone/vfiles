# VFiles 架构概览

本文档描述当前仓库中实际维护的实现：Rust 后端、SQLite 元数据存储、文件内容落地到本地存储、Vue 3 前端通过 HTTP API 与服务交互。

## 总体分层

```text
Browser / Vue UI
        |
        v
Rust HTTP (Axum)
        |
        v
Application Services (vfiles-app)
        |
        v
Repositories + Blob/Upload Store
        |
        v
SQLite + local filesystem
```

## 仓库结构

### Rust workspace

- `crates/vfiles-bin`
  - CLI 入口，提供 `serve`、`init`、`user`、`register`、`check`
- `crates/vfiles-http`
  - Axum 路由、DTO、错误映射、静态前端托管
- `crates/vfiles-app`
  - 业务服务：工作区、上传、历史、搜索、认证、分享、管理
- `crates/vfiles-domain`
  - 领域模型、ID 类型、仓储 trait、统一错误
- `crates/vfiles-infra-sqlite`
  - SQLite 仓储实现与迁移
- `crates/vfiles-infra-fs`
  - blob/upload 等文件系统存储实现
- `crates/vfiles-config`
  - 配置加载与路径计算
- `crates/vfiles-test-kit`
  - 集成测试辅助

### 前端

- `client/src/components`：文件浏览、上传、历史、共享等组件
- `client/src/stores`：Pinia 状态管理
- `client/src/services`：HTTP 调用封装
- `client/src/views`：页面级视图

## 运行时模型

### 存储

- SQLite 保存用户、条目、版本、快照、分享等元数据
- 文件内容按 blob 写入 `data/blobs`
- 上传中的分块数据写入 `data/uploads`
- 快照用于目录/版本历史回放

### HTTP 能力

`vfiles-http` 主要暴露这些路由组：

- `/api/auth`：登录、注册、退出、当前用户
- `/api/session`：bootstrap 与功能矩阵
- `/api/files`：目录树、上传、搜索、快照、内容读取、移动、删除
- `/api/history`：历史列表、diff、版本恢复
- `/api/download`：文件/目录下载
- `/api/share`：分享链接创建与管理
- `/api/admin`：管理员用户接口
- `/s/{code}`：公开分享下载

### 前端托管

服务启动时有两种模式：

- 外部静态资源：读取 `client/dist` 或 `VFILES_FRONTEND_DIST`
- 嵌入式：编译时启用 `embed` feature，把前端打进二进制

## 认证与多用户

- 会话通过 `auth_token` Cookie 维护
- 当认证开启且启用多用户命名空间时，每个登录用户会路由到自己的默认 namespace
- 管理员接口会校验角色；普通用户不能访问 `/api/admin`

## 关键设计点

### 目录树与历史

- 目录浏览只返回直属子项
- 写操作会生成版本或快照，以支持回放和差异查看
- 目录历史使用快照层，文件历史使用版本层

### 上传

- 支持单次 multipart 上传
- 支持分块上传：初始化、逐块上传、最终合并
- 上传完成后会写入条目版本并刷新历史/快照

### 分享

- 分享记录存储在数据库中
- 可以为文件或目录创建分享码
- 公开下载路径与带认证的管理路径分离

## 当前不再维护的部分

- 旧 TypeScript/Bun 服务端已从仓库移除
- Docker / Docker Compose / Nginx 配置不再作为当前仓库的一部分
- 相关历史规划文档保留在 `docs/`，仅作演进记录，不作当前实现说明
