# VFiles 部署指南

本指南以当前 Rust 后端为准。旧 TypeScript/Bun 后端的运行方式和环境变量已不再作为部署基线。

## 部署模式

VFiles 目前有两种推荐部署方式：

- 外部静态资源模式：Rust 服务启动时自动探测 `client/dist`，或通过 `VFILES_FRONTEND_DIST` 指向前端产物目录。
- 单文件模式：构建时启用 `embed` feature，把 `client/dist` 直接嵌入 `vfiles` 可执行文件。

仓库内已经移除了旧的 Docker / Docker Compose / Nginx 配置；当前文档只覆盖原生二进制部署方式。

## 前置要求

- Rust 工具链
- 系统已安装 `git`（服务端通过 git 子进程完成版本、历史和工作区操作）
- Bun：仅在本机重新构建前端时需要；如果直接使用现成的 `client/dist`，运行时不需要 Bun

## 环境变量

Rust 后端当前真实读取的核心配置如下：

```env
VFILES_HTTP_HOST=0.0.0.0
VFILES_HTTP_PORT=3000
VFILES_HTTP_PUBLIC_BASE_URL=https://files.example.com
VFILES_HTTP_CORS_ALLOWED_ORIGINS=https://files.example.com
# VFILES_HTTP_COOKIE_SECURE=true

VFILES_STORAGE_ROOT=./data
VFILES_DATABASE_PATH=./data/vfiles.db

VFILES_AUTH_ENABLED=true
VFILES_AUTH_ALLOW_REGISTER=true
VFILES_AUTH_COOKIE_SECRET=replace-with-a-random-secret-at-least-32-chars

# 功能开关（默认关闭；开启后前端会同步解锁对应入口）
VFILES_FEATURES_SEARCH_CONTENT=false

VFILES_FRONTEND_DIST=./client/dist
RUST_LOG=info
```

说明：

- `VFILES_HTTP_PUBLIC_BASE_URL` 决定对外可见的服务地址；默认情况下，登录/退出 cookie 是否带 `Secure` 也会跟随它的 scheme。
- `VFILES_HTTP_COOKIE_SECURE` 可显式覆盖 cookie 的 `Secure` 标记；如果公网仍走 `https://` 域名，但本地想直接用 `http://局域网IP:端口` 访问并登录，可临时设为 `false`。
- `VFILES_HTTP_CORS_ALLOWED_ORIGINS` 留空时，会默认回落到 `VFILES_HTTP_PUBLIC_BASE_URL` 的 origin；如果前端和 API 不同源，请显式写成逗号分隔列表。
- `VFILES_STORAGE_ROOT` 下会自动创建 `blobs`、`uploads`、`tmp`、`export`、`logs`、`backups` 等目录。
- `VFILES_FRONTEND_DIST` 在运行时用于外部静态资源托管；启用 `embed` feature 时，也可在编译期指定待嵌入目录。
- 仍兼容读取旧别名 `PUBLIC_BASE_URL`、`CORS_ORIGIN`、`HTTP_COOKIE_SECURE`、`AUTH_SECRET`、`ENABLE_AUTH`、`AUTH_ALLOW_REGISTER`，但新部署不建议继续使用旧名字。
- `VFILES_FEATURES_SEARCH_CONTENT` 控制**全文（内容）搜索**：默认关闭，因为它需要逐个读取并扫描文件内容，代价明显高于文件名搜索。开启后 `/api/session/bootstrap` 会把 `features.search_content` 置为 `true`，前端「高级搜索 → 全文搜索」才会解锁；服务端仍会对未开启时携带 `search_content=true` 的请求返回 403。
- 上传限额、分块大小、会话 TTL 等参数当前仍使用程序内建默认值，尚未开放成环境变量。

## 构建与启动

### 外部静态资源模式

先构建前端，再编译 Rust 服务：

```bash
cd client && bun install && bun run build
cd ..

cargo build -p vfiles-bin --release
./target/release/vfiles serve
```

默认会自动探测项目内的 `client/dist`。如果前端产物放在其他位置，可在启动前设置：

```bash
VFILES_FRONTEND_DIST=/path/to/dist ./target/release/vfiles serve
```

### 单文件模式

如果希望部署时只分发一个可执行文件，可以把前端嵌进二进制：

```bash
cd client && bun install && bun run build
cd ..

cargo build -p vfiles-bin --release --features embed
./target/release/vfiles serve
```

如果前端产物不在默认位置，可以在编译时指定：

```bash
VFILES_FRONTEND_DIST=/path/to/dist cargo build -p vfiles-bin --release --features embed
```

## 交付产物

当前仓库不再提供根目录的一键打包脚本。部署时请按实际模式自行组织交付物：

- 外部静态资源模式：发布 `target/release/vfiles` 与 `client/dist/`
- 单文件模式：发布启用 `embed` feature 构建出的 `target/release/vfiles`
- `.env` 可选，但生产环境通常建议保留一份显式配置
- `data/` 会在首次运行时自动初始化数据库和存储目录，也可以预先准备

运行方式：

```bash
./vfiles serve
```

Windows PowerShell：

```powershell
.\vfiles.exe serve
```

如果需要提前初始化数据库和目录，也可以先执行：

```bash
./vfiles init
```

## 注册为 systemd 服务

在 Linux 主机上，可以直接用内置 CLI 生成并注册 systemd unit：

```bash
sudo ./vfiles register -t systemd \
	--working-directory /srv/vfiles \
	--data-directory /var/lib/vfiles \
	--start
```

说明：

- `--working-directory` 会写入 unit 的 `WorkingDirectory=`，建议指向你的部署目录
- `--data-directory` 会写入 `VFILES_STORAGE_ROOT` 环境变量，用于显式指定数据根目录
- 如果只想注册服务但暂不启动，省略 `--start` 即可；命令会完成 `daemon-reload` 和 `enable`

服务进程支持优雅停机：收到 `SIGTERM`（`systemctl stop/restart`）或 `SIGINT`（Ctrl+C）
后会先停止接收新请求、等待在途请求结束，再关闭数据库连接池并以退出码 0 结束，日志中会
依次出现 `Shutdown signal received` 与 `VFiles server stopped`。

## 反向代理

如果通过 Nginx 或 Caddy 暴露服务，建议：

- 外网 HTTPS 终止后，把公开访问地址写入 `VFILES_HTTP_PUBLIC_BASE_URL`
- 如果浏览器端与 API 不同源，显式设置 `VFILES_HTTP_CORS_ALLOWED_ORIGINS`
- 下载接口转发时保留 Range 相关头
- 上传场景适当提高反向代理的请求体限制

## 运行验收

最小验收建议：

- `GET /api/health` 返回成功
- `./vfiles check` 可以完成健康检查
- 如果提供前端静态资源，访问根路径可以加载页面
- 上传、删除、移动等写操作可正常产生历史版本

## 维护任务

上传中断可能在 blob 目录留下「有文件但无元数据行」的孤儿文件（内容寻址，删除后若再次
上传相同内容会重新落盘，因此清理是安全的）：

```bash
# 默认保护期 1 小时，只清理早于保护期的孤儿 blob
./vfiles maintenance gc-blobs
# 自定义保护期（秒）
./vfiles maintenance gc-blobs --grace-seconds 600
```

输出 `Purged N orphaned blob(s), freed M bytes`。建议在业务低峰期执行；被任何版本或
快照引用的 blob 不会被删除。

历史快照会持续引用 blob（这是可恢复任意版本的前提），可用快照裁剪控制磁盘占用：

```bash
# 每个命名空间仅保留最新 50 个快照，并释放被删快照的 blob 引用
./vfiles maintenance prune-snapshots --keep 50
./vfiles maintenance prune-snapshots --keep 50 --older-than-days 90  # 再叠加时间窗口
```

裁剪会删除旧快照（不可再恢复到这些提交），请在确认不再需要旧历史后执行。

### 缩略图格式（AVIF 默认关闭）

缩略图会按请求的 `Accept` 协商输出格式，并声明 `Vary: accept`：

| 场景 | 输出 |
| --- | --- |
| `VFILES_THUMBNAIL_AVIF=true` 且客户端接受 `image/avif` | AVIF（体积最小，CPU 开销大） |
| 其他情况（含默认配置） | JPEG |

```bash
# 默认：只输出 JPEG
VFILES_THUMBNAIL_AVIF=false

# 开启 AVIF：实测 384px 缩略图 4.8KB vs JPEG 30.3KB，但首次编码约 2.0s（JPEG 0.004s），
# 结果会落盘缓存，只有首次请求付出代价
VFILES_THUMBNAIL_AVIF=true
```

> 服务端**不输出 WebP**：`image` crate 只提供无损 WebP 编码器，实测照片类缩略图
> 无损 WebP 达 160KB（JPEG 28KB），属于性能倒退。需要有损 WebP 需引入 libwebp（C 依赖）。

### 日志级别

默认输出 `info` 及以上级别（启动、维护任务、缩略图告警等），可用 `RUST_LOG` 覆盖：

```bash
RUST_LOG=debug ./vfiles serve          # 调试
RUST_LOG=vfiles_http=warn ./vfiles serve  # 只看 HTTP 层告警
```

### 健康检查与缩略图计数

`GET /api/health` 除 `status`/`timestamp` 外，还会返回缩略图的进程内计数，便于采集与排障：

```json
{
  "status": "ok",
  "timestamp": "2026-01-01T00:00:00Z",
  "thumbnail": {
    "cache_hits": 12,
    "generated": 5,
    "unsupported": 2,
    "failed": 1,
    "pruned_entries": 64,
    "pruned_bytes": 47424
  }
}
```

`unsupported` 表示格式不支持或源文件过大而被跳过，`failed` 表示解码/编码失败（两者都返回
415）；`pruned_*` 累计缓存回收量。计数自进程启动起累计，重启后归零。

### 缩略图缓存

网格视图的缩略图会按「blob + 尺寸」缓存在 `<存储根>/thumbnails`，按 mtime 回收最旧条目：

| 环境变量 | 默认值 | 说明 |
| --- | --- | --- |
| `VFILES_THUMBNAIL_CACHE_MAX_ENTRIES` | `2000` | 缓存条目上限 |
| `VFILES_THUMBNAIL_CACHE_MAX_MB` | `256` | 缓存总字节上限（MB） |

两者任一超限都会触发回收（每 64 次写入检查一次），清理到上限的 80% 为止，并始终保留最新
一条，避免单张超大缩略图导致缓存被反复清空。日志（`RUST_LOG=info`）会输出
`pruned thumbnail cache removed=.. removed_bytes=.. remaining_entries=.. remaining_bytes=..`。
支持的源格式：JPEG / PNG / GIF / WebP / BMP / TIFF / ICO / QOI。

### 周期性维护

除了手动执行，也可以让服务在后台按周期自动维护。**默认关闭**（涉及不可逆删除，需显式开启）：

| 环境变量 | 默认值 | 说明 |
| --- | --- | --- |
| `VFILES_MAINTENANCE_ENABLED` | `false` | 是否启用周期性维护 |
| `VFILES_MAINTENANCE_INTERVAL_SECONDS` | `86400` | 执行间隔（秒），最小 60 |
| `VFILES_MAINTENANCE_INITIAL_DELAY_SECONDS` | `300` | 启动后首次执行前等待（秒），不超过间隔且最多 5 分钟 |
| `VFILES_MAINTENANCE_BLOB_GRACE_SECONDS` | `3600` | 孤儿 blob 保护期（秒） |
| `VFILES_MAINTENANCE_SNAPSHOT_KEEP` | `0` | 每个命名空间保留的快照数；`0`（默认）表示不裁剪快照 |
| `VFILES_MAINTENANCE_SNAPSHOT_MAX_AGE_DAYS` | `0` | 快照最长保留天数；`0`（默认）表示不按时间裁剪。与数量策略是「与」关系：只删除既超出 `KEEP` 又早于该天数的快照 |

```bash
export VFILES_MAINTENANCE_ENABLED=true
export VFILES_MAINTENANCE_SNAPSHOT_KEEP=50   # 需要控制历史占用时再打开
```

每轮先按需裁剪快照、再回收孤儿 blob，日志（`RUST_LOG=info`）会输出
`Periodic maintenance finished pruned_snapshots=.. released_blobs=.. purged_blobs=.. freed_bytes=..`；
单轮失败只记录告警并在下个周期重试，不影响服务。停机（SIGTERM/SIGINT）时会先停止维护任务、
再关闭数据库连接池。
