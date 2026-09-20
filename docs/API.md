# VFiles HTTP API

本文档描述当前 Rust + Axum 实现暴露的 HTTP 接口。所有受保护接口默认挂在 `/api` 之下，认证通过 `auth_token` Cookie 完成。

## 返回风格

当前接口有两类返回风格：

- 直接返回 DTO / 数组：例如目录列表、用户列表、分享列表
- 兼容包装：部分旧前端兼容接口仍返回 `{ success, data }`
- 下载与内容接口直接返回二进制流

## 健康检查

- `GET /api/health`
- `GET /api/ready`

## 会话与认证

- `GET /api/session/bootstrap`
  - 返回认证开关、当前用户、功能矩阵与活动工作区
- `POST /api/auth/login`
  - Body: `username_or_email`, `password`
  - 成功后写入 `auth_token` Cookie
- `POST /api/auth/register`
  - Body: `username`, `email`, `password`
- `POST /api/auth/logout`
- `GET /api/auth/me`

## 文件树与目录操作

- `GET /api/files/tree`
  - 列出根目录直属子项
- `GET /api/files/tree/{path}`
  - 列出指定目录直属子项
  - Query: `commit` 可选，值为 snapshot/version 兼容标识
- `GET /api/files/list` / `GET /api/files/list/{path}`
  - 分页列出目录直属子项（大目录推荐使用）
  - Query: `limit`（默认 200，夹取到 1..1000）、`offset`（默认 0）、`commit` 可选
  - 响应：`{ items, total, limit, offset, has_more }`
- `POST /api/files/directories`
  - Body: `path`
  - 创建目录；会自动补齐缺失父目录
- `POST /api/files/move`
  - Body: `from`, `to`, `message?`
- `DELETE /api/files?path=...&message=...`

## 文件内容与下载

- `GET /api/files/content?path=...&commit=...`
  - 返回原始文件流，适用于预览或直接读取内容
- `GET /api/download?path=...&commit=...`
  - 下载单文件
- `GET /api/download/folder?path=...&commit=...`
  - 下载目录 ZIP

## 上传

### 单请求上传

- `POST /api/files/upload`
  - `multipart/form-data`
  - 字段：`file`, `path?`, `message?`

### 分块上传

- `POST /api/files/upload/init`
  - JSON: `path`, `filename`, `size`, `chunk_size?`, `last_modified?`, `mime?`
  - 返回：`upload_id`, `chunk_size`, `total_chunks`
- `PUT /api/files/upload/chunks/{upload_id}/{chunk_index}`
  - Body: 当前分块的二进制内容
- `POST /api/files/upload/complete/{upload_id}`
  - JSON: `message?`

## 快照与历史

- `POST /api/files/snapshots`
  - Body: `message?`
- `GET /api/history?path=...&limit=...`
  - 文件路径返回版本历史；目录路径返回目录级快照历史
- `GET /api/history/diff?path=...&commit=...&parent=...`
  - 返回纯文本 diff
- `POST /api/history/restore`
  - Body: `path`, `commit`, `message?`

## 搜索

- `GET /api/files/search`
  - Query:
    - `q` 必填
    - `search_files` 布尔，可选
    - `search_content` 布尔，可选
    - `path` 可选，限定目录前缀
    - `type` 可选：`all` / `file` / `directory`
    - `limit` / `offset` 可选

## 分享

- `POST /api/share/shares`
  - Body: `path`, `expires_at?`（RFC3339）
- `GET /api/share/shares`
- `GET /api/share/shares/{code}`
- `DELETE /api/share/shares/{code}`
- `GET /api/share/shares/{code}/download`
- `GET /s/{code}`
  - 对外公开下载入口

## 管理员接口

这些接口要求管理员或具备管理能力的账号：

- `GET /api/admin/users?page=1&page_size=20`
- `POST /api/admin/users`
- `GET /api/admin/users/{user_id}`
- `PUT /api/admin/users/{user_id}`
- `DELETE /api/admin/users/{user_id}`
- `POST /api/admin/users/{user_id}/revoke-sessions`
- `POST /api/admin/users/{user_id}/reset-password`

## 说明

- 分享、历史、内容搜索等能力受服务端 feature matrix 控制；关闭后会返回 `403`。
- `path` 参数统一使用相对仓库根的规范化路径，不允许目录穿越。
- 如果需要更精确的返回字段，可直接对照 `crates/vfiles-http/src/dto.rs` 与对应 route 实现。
