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

### 验证基线（本轮实测）

- `cargo test --workspace`：通过。
- `client` 单测：8 个文件 / 23 个用例通过（本轮新增 3 个测试文件）。
- `vue-tsc --noEmit`：无错误；`eslint .`：无告警。
- `bun run build`：成功；服务端 `--features embed` 构建成功，冒烟测试
  `/api/health`、`/api/files/tree`、上传与 `/api/files/content` 均返回预期结果。

### 主要发现

| 类别   | 问题                                                                          | 影响                      | 状态          |
| ------ | ----------------------------------------------------------------------------- | ------------------------- | ------------- |
| 性能   | 预览按需加载完整 `highlight.js`（约 190 种语言），chunk 达 914KB / gzip 304KB | 首次代码预览加载缓慢      | `[x]` 见 §2.1 |
| 性能   | 全量 Bulma CSS 约 678KB / gzip 66KB，渲染阻塞                                 | 首屏 CSS 体积大           | `[ ]` 见 §3.1 |
| 性能   | 缺少服务端缩略图接口，网格视图直接拉取原图                                    | 大图目录流量偏高          | `[ ]` 见 §3.2 |
| 交互   | 文件列表不支持排序                                                            | 与主流云盘差异明显        | `[x]` 见 §2.2 |
| 交互   | 仅有表格视图，无网格/缩略图视图                                               | 图片目录浏览体验差        | `[x]` 见 §2.3 |
| 稳定性 | `FileBrowser.vue` 单文件近 3000 行，职责过载                                  | 维护与回归风险            | `[ ]` 见 §3.3 |
| 稳定性 | 缩略图缓存缺少服务端降级与压力上限的可观测性                                  | 极端目录下的内存/带宽未知 | `[~]` 见 §3.2 |

## 2. 本轮迭代（已完成）

### 2.1 精简预览高亮依赖

- 新增 `client/src/utils/highlight.ts`，改为按需加载官方维护的
  `highlight.js/lib/common` 子集，并在 `FileBrowser.vue`、`VersionHistory.vue`
  中复用，消除重复实现。
- 实测产物：预览相关 chunk 由 **914KB（gzip 304KB）降至 152KB（gzip 52KB）**。

### 2.2 可持久化的排序

- 新增 `client/src/utils/fileSort.ts`：按名称（`Intl.Collator` 数字感知，兼容中文）、
  修改时间、大小、类型排序，支持升降序与“文件夹置顶”，并保证快捷项（`.`/`..`）
  始终位于列表顶部；纯函数便于测试。
- 新增 `client/src/stores/fileView.store.ts`：视图模式、排序字段/方向、
  文件夹置顶、缩略图尺寸持久化到 `localStorage`，损坏数据自动回退默认值。
- `FileBrowser.vue` 的目录列表与搜索结果统一接入排序。

### 2.3 网格视图与缩略图

- 新增 `FileGrid.vue` / `FileCard.vue`：卡片式网格布局，支持多选、命名高亮、
  每卡片操作菜单（预览、历史、重命名、移动、下载、分享、删除）。
- 新增 `client/src/stores/thumbnails.store.ts`：`IntersectionObserver` 懒加载、
  并发上限 4、缓存上限 160 并回收 objectURL、超大源文件跳过缩略图。
- 新增 `ViewOptions.vue` 工具栏：列表/网格切换、排序字段与方向、文件夹置顶、
  缩略图尺寸调节。
- 网格视图隐藏 `.`/`..` 快捷项，导航交由面包屑与“上一级”按钮，贴近主流云盘。

### 2.4 测试

- `tests/fileSort.test.ts`、`tests/fileView.store.test.ts`、`tests/FileGrid.test.ts`，
  并在 `tests/FileBrowser.test.ts` 增加网格渲染 + 排序的集成用例。

## 3. 后续迭代计划（按优先级）

### 3.1 首屏 CSS 体积（性能，高）

- 将 `@import "bulma/css/bulma.min.css"` 改为按需引入 Bulma 的 Sass 局部模块，
  只保留实际使用的组件（navbar、table、button、modal、notification、tag 等）。
- 验收：`dist/assets/index-*.css` 体积显著下降，页面视觉无回归。

### 3.2 服务端缩略图（性能 + 稳定性，高）

- 新增缩略图接口（如 `GET /api/files/thumbnail?path=...&size=...`），
  服务端生成并缓存小尺寸图片，网格视图不再下载原图。
- 前端 `thumbnails.store.ts` 切换为消费该接口，并在请求失败时回退到类型图标。
- 验收：图片目录首屏流量与内存占用可量化下降。

### 3.3 `FileBrowser.vue` 拆分（稳定性，中）

- 抽出搜索、批量操作、下载队列、目录管理、预览为独立 composable / 组件，
  把主组件收敛为编排层。
- 验收：单文件行数显著下降，已有测试保持通过并补充拆分后的单元测试。

### 3.4 交互增强（中，对齐主流云盘）

- 列表视图支持点击表头排序；键盘多选（Shift/Ctrl）与快捷键（Delete、F2、Enter）。
- 面包屑支持拖放移动与路径下拉快速跳转。
- 上传/下载与网格视图的空状态、加载骨架屏统一。

### 3.5 稳定性与可观测性（中）

- 为缩略图缓存增加命中率/失败计数日志，便于定位问题。
- 前端错误边界与 API 失败重试策略梳理，避免重复请求。
