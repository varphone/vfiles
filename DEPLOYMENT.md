# VFiles 部署指南

本指南覆盖本地/服务器部署的推荐方式（Bun + 前端静态资源由后端托管）。

## 前置要求

- Bun（与项目 `bun.lockb` 兼容的版本）
- 系统已安装 `git`（服务端通过 git 子进程完成读取/写入/历史查询）

## 环境变量

常用配置（可放在 `.env`）：

```env
PORT=3000
REPO_PATH=./data
REPO_MODE=worktree
CORS_ORIGIN=*
NODE_ENV=production

# 上传
UPLOAD_CHUNK_SIZE=5242880
UPLOAD_MAX_CHUNK_SIZE=20971520
UPLOAD_TEMP_DIR=./.vfiles_uploads
UPLOAD_SESSION_TTL_MS=86400000

# 下载缓存
DOWNLOAD_CACHE_DIR=./.vfiles_download_cache
DOWNLOAD_CACHE_TTL_MS=21600000

# Git 查询缓存
GIT_QUERY_CACHE_ENABLED=true
GIT_QUERY_CACHE_TTL_MS=300000
GIT_QUERY_CACHE_LIST_FILES_MAX=300
GIT_QUERY_CACHE_FILE_HISTORY_MAX=300
GIT_QUERY_CACHE_LAST_COMMIT_MAX=3000
GIT_QUERY_CACHE_DEBUG=false

# Git LFS
ENABLE_GIT_LFS=true
GIT_LFS_TRACK_PATTERNS=*.png,*.jpg,*.zip

# 安全
ALLOWED_PATH_PREFIXES=
RATE_LIMIT_ENABLED=true
RATE_LIMIT_WINDOW_MS=60000
RATE_LIMIT_MAX=120
```

## 生产构建与启动

```bash
bun install
cd client && bun install

# 构建前端
bun run build

# 启动服务（生产环境建议设置 NODE_ENV=production）
bun run start
```

默认访问：`http://localhost:3000`

## 打包发布（运行时不依赖 node_modules）

目标：产出一个“可发布目录”，运行时只需要：
- 服务端可执行文件（由 `bun build --compile` 生成）
- 前端静态资源目录 `client/dist/`
- （可选）`.env` 配置文件与数据目录（`data/` 或 `data.git/`）

> 说明：此方式不会在目标机器上依赖 `node_modules`。但仍然需要系统安装 `git`（VFiles 通过 git 子进程工作）。

### 1) 构建前端静态资源

在项目根目录执行：

```bash
bun install
cd client && bun install
bun run build
```

构建产物会生成在：`client/dist/`

### 2) 编译服务端为可执行文件

在项目根目录执行：

```bash
bun build --compile server/src/index.ts --outfile dist/vfiles
```

- Windows 上一般会生成 `dist/vfiles.exe`
- Linux/macOS 上会生成对应平台的可执行文件

> 注意：可执行文件需要在“目标平台”上构建（Windows 构建 Windows 版，Linux 构建 Linux 版）。

### 3) 组装发布目录

推荐目录结构（示例）：

```
release/
	vfiles(.exe)
	.env                # 可选
	client/dist/         # 必须（生产模式下后端会托管该目录）
	data/                # 可选（worktree 模式）
	data.git/            # 可选（bare 模式）
	.vfiles_uploads/     # 运行时自动创建（可选预建）
	.vfiles_download_cache/
```

将以下内容复制到 `release/`：
- `dist/vfiles(.exe)`
- `client/dist/`
- （可选）`.env`（可从 `.env.example` 改名生成）

### 4) 运行

在 `release/` 目录下启动（确保 `NODE_ENV=production`）：

```bash
NODE_ENV=production ./vfiles
```

或在 Windows PowerShell：

```powershell
$env:NODE_ENV="production"; .\vfiles.exe
```

验收标准：
- 访问 `/health` 返回 `ok`
- 访问根路径能加载前端页面（依赖 `client/dist/index.html` 存在）
- `/api/files?path=` 返回 `success=true`

## REPO_MODE 建议

- `worktree`：磁盘上能直接看到文件，适合本地/小规模使用。
- `bare`：仅存 git 对象库（无工作区文件），适合更关注磁盘占用与“只通过 API 访问”的部署。

### 从 worktree 迁移到 bare（可选）

如果已有 `data/` 工作区仓库，想迁移到 `data.git/`：

```bash
git clone --bare data data.git
```

然后设置：

```env
REPO_MODE=bare
REPO_PATH=./data.git
```

## 反向代理（可选）

若通过 Nginx/Caddy 暴露服务，建议：
- 转发 `GET /api/download`、`GET /api/download/folder` 时保留 Range 头
- 适当提高请求体限制（上传）

## 运行健康检查

- 访问首页应能加载
- `GET /api/files?path=` 返回 `success=true`
- 上传/删除/移动等写操作可正常产生 git commit
