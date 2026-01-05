# VFiles API 文档

本文档描述 VFiles 后端 HTTP API（Bun + Hono）。

## 基础信息

- Base URL：`http://localhost:3000`
- 响应格式（JSON）：

```json
{ "success": true, "data": {} }
```

失败时：

```json
{ "success": false, "error": "错误信息" }
```

## 通用约定

### path 参数

- `path` 为仓库根目录的相对路径。
- 目录用 `""` 表示仓库根。
- 服务端会做路径标准化与安全校验（禁止路径穿越）。

### commit 参数

部分接口支持 `commit`：

- 为空表示当前版本（worktree 下为工作区；bare 下等同 `HEAD`）。
- 非空时应为 git commit hash（完整或短 hash），用于“在某个历史版本下浏览/下载”。

---

## 文件列表与内容

### GET /api/files

获取目录下的文件列表。

Query:

- `path`：目录路径（可选，默认 `""`）
- `commit`：可选，指定目录浏览所基于的版本

返回：`data` 为文件/目录数组（包含 name/path/type/size/mtime/lastCommit 等字段）。

### GET /api/files/content

获取文件内容（支持历史版本）。

Query:

- `path`：文件路径（必填）
- `commit`：可选

返回：

- 成功时 `data` 为文件内容/元信息（不同类型可能不同）；
- 对二进制文件建议使用下载接口。

---

## 上传

### POST /api/files/upload

普通上传（multipart/form-data）。

FormData:

- `file`：文件（必填）
- `path`：目标目录（可选，默认根）
- `message`：提交信息（可选）

返回：`data.path`、`data.commit`。

### POST /api/files/upload/init

分块上传初始化/续传探测。

JSON Body:

- `path`：目标目录（可选）
- `filename`：文件名（必填）
- `size`：文件大小字节数（必填）
- `mime`：可选
- `lastModified`：可选

返回：

- `uploadId`、`chunkSize`、`totalChunks`
- `received`：已存在的分块索引列表（用于断点续传）

### POST /api/files/upload/chunk?uploadId=...&index=...

上传单个分块。

- Query: `uploadId`（必填）, `index`（必填）
- Body: 二进制（`application/octet-stream`）

### POST /api/files/upload/complete

合并分块并提交。

JSON Body:

- `uploadId`（必填）
- `message`：提交信息（可选）

返回：`data.path`、`data.commit`。

---

## 删除 / 移动 / 创建目录

### DELETE /api/files

删除文件。

Query:

- `path`：必填
- `message`：可选

### POST /api/files/move

移动/重命名文件或目录。

JSON Body:

- `from`：必填
- `to`：必填
- `message`：可选

### POST /api/files/dir

创建目录。

JSON Body:

- `path`：必填
- `message`：可选

---

## 历史

### GET /api/history

获取文件历史；当 `path=""` 时表示获取仓库根的提交历史（用于目录级版本浏览）。

Query:

- `path`：必填（允许空字符串）
- `limit`：可选，默认 50，范围 1~200

### GET /api/history/diff

获取某个版本的文本 diff。

Query:

- `path`：必填
- `commit`：必填
- `parent`：可选

返回：纯文本 diff（`text/plain`）。

### GET /api/history/commit/:hash

获取提交详情。

---

## 下载

### GET /api/download

下载文件（支持 `commit`，并对当前版本尽可能提供 Range 续传）。

Query:

- `path`：必填
- `commit`：可选

返回：文件流（`application/octet-stream`）。

### GET /api/download/folder

下载文件夹 ZIP（支持 `commit`）。

Query:

- `path`：可选（默认根）
- `commit`：可选（为空表示 `HEAD`）

返回：ZIP 流（`application/zip`）。

---

## 搜索

### GET /api/search

搜索文件。

Query:

- `q`：关键字（必填）
- `mode`：`name`（默认）或 `content`
- `type`：`all`（默认）/ `file` / `directory`
- `path`：可选，限定在某个目录下搜索

返回：匹配结果数组。
