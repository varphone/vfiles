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

## 侧栏聚合与收藏

`GET /api/files/overview`（需要登录）返回条目统计、**按类型聚合的占用**与最近文件：

```json
{
  "file_count": 6,
  "directory_count": 3,
  "total_bytes": 54168,
  "categories": [
    { "category": "video", "bytes": 40000, "file_count": 1 },
    { "category": "other", "bytes": 9000, "file_count": 1 },
    { "category": "document", "bytes": 4209, "file_count": 2 },
    { "category": "image", "bytes": 959, "file_count": 2 }
  ],
  "recent_files": [ ... ]
}
```

`categories` 的分类取值：`document`（文本 / PDF / Office）、`image`、`video`、`audio`、`other`
（MIME 缺失或其它），按字节数倒序排列，只统计文件的当前版本。分类聚合失败时该字段返回空数组，
不影响总数与最近文件。

- `GET /api/files/overview`
  - 返回 `file_count`、`directory_count`、`total_bytes` 与 `recent_files`（最近 8 条），
    用于侧栏「存储用量 / 最近更新」
- `GET /api/files/favorites`
  - 返回收藏条目 `items[{path,name,kind}]`
- `POST /api/files/favorites`
  - Body: `path`；幂等，返回最新列表
- `DELETE /api/files/favorites?path=...`
  - 幂等，返回最新列表
  - 收藏按条目 ID 记录：重命名/移动后仍然有效，条目删除后自动清除

## FTP 导入信息

`GET /api/files/ftp-info`（需要登录）

返回当前部署的 FTP 连接参数，便于前端展示与排错；**不包含任何口令或证书内容**。

```json
{
  "enabled": true,
  "host": "files.example.com",
  "host_source": "passive_host",
  "remote_reachable": true,
  "port": 2121,
  "passive_ports": { "start": 50000, "end": 50100 },
  "tls": { "enabled": true, "required": true },
  "example_command": "curl --ftp-ssl -T 本地文件 ftp://files.example.com:2121/目录/",
  "path_mapping": "登录后 / 即该用户的命名空间根目录"
}
```

FTP 默认启用；显式关闭（`VFILES_FTP_ENABLED=false`）或认证关闭时 `enabled` 为 `false`，
`example_command` 为 `null`。未登录访问返回 401。

`host` 按「客户端最可能连得上」的顺序选取，并用 `host_source` 说明来源：
`passive_host`（`VFILES_FTP_PASSIVE_HOST`）→ `request`（当前请求的 Host 头）→
`public_base_url` → `bind_address` → `detected_address`（本机网卡地址）→ `loopback`。
`remote_reachable=false` 表示只能给出回环地址（例如服务器没有可用网卡地址或全部来源都是
localhost），前端会提示「仅本机可访问」。通配地址（`0.0.0.0`/`::`）不会出现在 `host` 中；
IPv6 字面量在 `example_command` 中带方括号。
运行计数（会话数、上传/下载字节、快照提交量）在 `GET /api/health` 的 `ftp` 字段中。

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
