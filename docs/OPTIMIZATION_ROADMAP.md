# VFiles 优化路线图（审计 + 迭代记录）

本文档记录对当前设计与实现的审计结论，以及按“稳定性 / 性能 / 用户交互”三个方向的
优化迭代计划。每一轮迭代完成后在此更新状态，便于持续跟踪。

状态标记：`[x] 已完成`、`[~] 进行中`、`[ ] 待办`。

## 1. 当前状态审计

### 架构

- 后端：Rust workspace 分层为 `vfiles-domain`（领域模型/仓储接口）、`vfiles-app`
  （用例编排）、`vfiles-http`（Axum 路由与中间件）、`vfiles-infra-sqlite` /
  `vfiles-infra-fs`（存储实现）、`vfiles-bin`（CLI 与启动）。分层清晰、边界明确。
- 前端：Vue 3 + Pinia + Bulma，服务端为前端提供 `/api/*`，生产环境通过
  `embed` feature 将 `client/dist` 编入二进制。
- 数据：SQLite（WAL）+ 内容寻址 blob 存储 + 快照/版本历史。

### 验证基线（round 2 实测）

- `cargo test --workspace`：通过（HTTP 集成测试 56 个，含 3 个新增缩略图用例）。
- `cargo clippy --workspace --all-targets`：无告警。
- `client` 单测：8 个文件 / 23 个用例通过。
- `vue-tsc --noEmit`：无错误；`eslint .`：无告警。
- `bun run build`：成功；服务端 `--features embed` 构建成功。冒烟测试覆盖
  `/api/files/thumbnail` 生成、磁盘缓存、`ETag` 304、尺寸钳制、非图片 415、
  以及 `/api/files/content` 原图不受影响。

### 主要发现

| 类别   | 问题                                                                          | 影响                      | 状态          |
| ------ | ----------------------------------------------------------------------------- | ------------------------- | ------------- |
| 性能   | 预览按需加载完整 `highlight.js`（约 190 种语言），chunk 达 914KB / gzip 304KB | 首次代码预览加载缓慢      | `[x]` 见 §2.1 |
| 性能   | 全量 Bulma CSS 约 678KB / gzip 66KB，渲染阻塞                                 | 首屏 CSS 体积大           | `[ ]` 见 §3.1 |
| 性能   | 缺少服务端缩略图接口，网格视图直接拉取原图                                    | 大图目录流量偏高          | `[x]` 见 §2.5 |
| 交互   | 文件列表不支持排序                                                            | 与主流云盘差异明显        | `[x]` 见 §2.2 |
| 交互   | 仅有表格视图，无网格/缩略图视图                                               | 图片目录浏览体验差        | `[x]` 见 §2.3 |
| 稳定性 | `FileBrowser.vue` 单文件近 3000 行，职责过载                                  | 维护与回归风险            | `[ ]` 见 §3.3 |
| 稳定性 | 缩略图缺少尺寸/像素上限与磁盘缓存                                             | 极端目录下的内存/带宽未知 | `[x]` 见 §2.5 |

## 2. 迭代记录（已完成）

### 2.1 精简预览高亮依赖（round 1）

- 新增 `client/src/utils/highlight.ts`，改为按需加载官方维护的
  `highlight.js/lib/common` 子集，并在 `FileBrowser.vue`、`VersionHistory.vue`
  中复用，消除重复实现。
- 实测产物：预览相关 chunk 由 **914KB（gzip 304KB）降至 152KB（gzip 52KB）**。

### 2.2 可持久化的排序（round 1）

- 新增 `client/src/utils/fileSort.ts`：按名称（`Intl.Collator` 数字感知，兼容中文）、
  修改时间、大小、类型排序，支持升降序与“文件夹置顶”，并保证快捷项（`.`/`..`）
  始终位于列表顶部；纯函数便于测试。
- 新增 `client/src/stores/fileView.store.ts`：视图模式、排序字段/方向、
  文件夹置顶、缩略图尺寸持久化到 `localStorage`，损坏数据自动回退默认值。
- `FileBrowser.vue` 的目录列表与搜索结果统一接入排序。

### 2.3 网格视图（round 1）

- 新增 `FileGrid.vue` / `FileCard.vue`：卡片式网格布局，支持多选、命名高亮、
  每卡片操作菜单（预览、历史、重命名、移动、下载、分享、删除）。
- 新增 `ViewOptions.vue` 工具栏：列表/网格切换、排序字段与方向、文件夹置顶、
  缩略图尺寸调节。
- 网格视图隐藏 `.`/`..` 快捷项，导航交由面包屑与“上一级”按钮，贴近主流云盘。

### 2.4 前端测试（round 1）

- `tests/fileSort.test.ts`、`tests/fileView.store.test.ts`、`tests/FileGrid.test.ts`，
  并在 `tests/FileBrowser.test.ts` 增加网格渲染 + 排序的集成用例。

### 2.5 服务端缩略图（round 2）

- 后端新增 `GET /api/files/thumbnail?path=...&size=...&commit=...`：
  - 在 `vfiles-http` 中按需解码、等比缩放并统一编码为不透明 JPEG
    （透明度合成到白底），通过 `spawn_blocking` 执行，避免阻塞异步运行时。
  - 磁盘缓存 `<storage_root>/thumbnails/<blob_id>-<size>.jpg`，先写临时文件再
    `rename`，并发读不会拿到半成品；尺寸钳制在 64–512。
  - 提供 `ETag` 与 `Cache-Control: private, max-age=604800`，支持 `If-None-Match` 返回 304。
  - 源文件超过 40MB 或像素/边长超限时跳过（防解压炸弹），非图片返回 415。
  - `FileContentBytes` 增加 `blob_id`，作为缓存键，避免与路径耦合。
- 前端移除 JS 侧 `thumbnails.store.ts`，`FileCard` 直接使用 `<img loading="lazy">`
  指向该接口，复用浏览器原生懒加载与 HTTP 缓存；加载失败回退类型图标。
- 实测：800×600 PNG → 128×96 JPEG（1.7KB），请求 512 时返回 512×384，
  重复请求命中磁盘缓存，`If-None-Match` 返回 304，原图接口不受影响。

## 3. 后续迭代计划（按优先级）

### 3.1 首屏 CSS 体积（性能，高）

- 将 `@import "bulma/css/bulma.min.css"` 改为按需引入 Bulma 的 Sass 局部模块，
  只保留实际使用的组件（navbar、table、button、modal、notification、tag 等）。
- 验收：`dist/assets/index-*.css` 体积显著下降，页面视觉无回归。

### 3.2 缩略图缓存治理（性能 + 稳定性，中）

- 为 `thumbnails/` 增加容量上限与 LRU/过期清理，并记录命中率与失败计数日志。
- 支持 `AVIF`/`TIFF` 等更多格式，或按需返回 WebP。

### 3.3 `FileBrowser.vue` 拆分（稳定性，中）

- 抽出搜索、批量操作、下载队列、目录管理、预览为独立 composable / 组件，
  把主组件收敛为编排层。
- 验收：单文件行数显著下降，已有测试保持通过并补充拆分后的单元测试。

### 3.4 交互增强（中，对齐主流云盘）

- 列表视图支持点击表头排序；键盘多选（Shift/Ctrl）与快捷键（Delete、F2、Enter）。
- 面包屑支持拖放移动与路径下拉快速跳转。
- 上传/下载与网格视图的空状态、加载骨架屏统一。

### 3.5 稳定性与可观测性（中）

- 前端错误边界与 API 失败重试策略梳理，避免重复请求。
- 图片解码失败、超大文件跳过等场景补充计数指标。
