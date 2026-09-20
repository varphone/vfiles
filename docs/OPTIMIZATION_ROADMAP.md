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

### 验证基线（round 14 实测）

- `cargo test --workspace`：通过（HTTP 集成 59 个 + `vfiles-http` 单元 9 个）。
- `cargo clippy --workspace --all-targets`：无告警。
- `client` 单测：18 个文件 / 89 个用例通过。
- `vue-tsc --noEmit`：无错误；`eslint .`：无告警。
- `bun run build`：成功（预压缩 12 组）；冒烟验证预览导航产物标记。

### 主要发现

| 类别   | 问题                                                      | 影响                       | 状态          |
| ------ | --------------------------------------------------------- | -------------------------- | ------------- |
| 性能   | 预览按需加载完整 `highlight.js`（约 190 种语言），914KB   | 首次代码预览加载缓慢       | `[x]` 见 §2.1 |
| 性能   | 全量 Bulma CSS 约 678KB，渲染阻塞                         | 首屏 CSS 体积大            | `[x]` 见 §2.6 |
| 性能   | 静态前端资源完全未压缩，首屏 CSS/JS 明文传输              | 首屏传输体积是压缩后的 10x | `[x]` 见 §2.7 |
| 性能   | 缺少服务端缩略图接口，网格视图直接拉取原图                | 大图目录流量偏高           | `[x]` 见 §2.5 |
| 交互   | 文件列表不支持排序                                        | 与主流云盘差异明显         | `[x]` 见 §2.2 |
| 交互   | 仅有表格视图，无网格/缩略图视图                           | 图片目录浏览体验差         | `[x]` 见 §2.3 |
| 稳定性 | `FileBrowser.vue` 单文件近 3000 行，职责过载              | 维护与回归风险             | `[ ]` 见 §3.3 |
| 稳定性 | 缩略图缺少尺寸/像素上限与磁盘缓存                         | 极端目录下的内存/带宽未知  | `[x]` 见 §2.5 |
| 稳定性 | 暗色系统下 Bulma 变量变暗而自定义样式仍为浅色，视觉不一致 | 深色系统用户观感异常       | `[x]` 见 §2.6 |

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
  重复命中磁盘缓存，`If-None-Match` 返回 304，原图接口不受影响。

### 2.6 Bulma 按需引入（round 3）

- 新增 `client/src/styles/bulma.scss`，按实际使用的类名只引入所需局部模块
  （保留元素/表单/helpers/navbar/dropdown/modal/card/breadcrumb/tabs 等，
  去除 menu/panel/message/pagination/grid/hero/skeleton）。
- `App.vue` 由 `@import "bulma/css/bulma.min.css"` 改为 `@use "./styles/bulma"`，
  样式块启用 `lang="scss"`。
- 固定为浅色主题：原实现引入 Bulma 的 `prefers-color-scheme: dark` 变量，
  而自定义样式全是浅色，会在深色系统上产生“深色组件 + 浅色面板”的不一致；
  暗色主题作为独立事项列入 §3.6。
- 产物：`index-*.css` 由 **678.54KB 降至 416.72KB（gzip 66.63KB → 41.61KB）**。
- 校验：脚本比对“源码中出现的 Bulma 类名”与“构建产物中的类名”，无缺失
  （`dropdown-trigger` 在 Bulma 1.0 本身无样式，属正常）。

### 2.7 静态资源与响应压缩（round 3）

- 复查时发现 `CompressionLayer` 只挂在 `/api` 子路由上，静态前端资源（`/`、
  `/assets/*`）**完全没有压缩**，首屏 CSS/JS 以数百 KB 明文传输——比 CSS 体积本身
  影响更大。
- 将压缩层提升到整个路由，并使用 `DefaultPredicate` + 排除音频/视频/PDF/压缩包/
  `application/octet-stream`，避免对已压缩内容做无谓 CPU 开销；`Content-Range`
  响应由 tower-http 自动跳过。
- 实测（真实产物，`Accept-Encoding: br`）：`index.css` 416,720B → 39,333B，
  主 JS → 61,795B，`index.html` → 328B；`image/jpeg` 缩略图不压缩；
  文本内容以 br 传输并可正常解码；`Range` 请求返回 206 且不带 `content-encoding`。

### 2.8 列表排序与键盘操作（round 4）

- 列表视图表头可点击排序：点击字段切换排序字段，重复点击同字段切换升降序，
  并设置 `aria-sort` 与排序指示箭头（与 `ViewOptions` 共用同一份持久化状态）。
- 表头新增“全选当前视图”复选框，支持 `indeterminate` 半选状态；`Ctrl/⌘+A`
  自动进入批量模式并全选（快捷项 `.`/`..` 不参与）。
- 全局快捷键：`Esc` 逐层退出（高级搜索 → 预览 → 批量 → 选择）、`Ctrl/⌘+A` 全选、
  `Delete`/`Backspace` 删除选中或高亮项、`F2` 重命名、`Enter` 打开/预览；
  输入框、下拉框与弹窗内不触发。
- 桌面状态栏补充快捷键提示，提升可发现性。
- 测试：`FileList`（表头排序事件、`aria-sort`、半选/全选）与 `FileBrowser`
  （`Ctrl+A` 全选、`Esc` 退出、输入框内不触发）新增用例。

### 2.9 右键菜单与多选（round 5）

- 列表行与网格卡片支持右键菜单（`ContextMenu.vue`，Teleport + 视口内定位 +
  点击外部/`Esc`/滚动关闭），菜单项按文件类型区分（目录：打开/新建子目录；
  文件：预览/历史），并复用重命名、移动、下载、分享、删除动作。
- 支持 `Ctrl/⌘` 点击加选与 `Shift` 点击范围选择（自动进入批量模式）；普通点击会
  记录锚点，因此“先点一项，再 Shift 点另一项”也能形成范围选择。
- 右键未选中项时，在批量模式下切换为仅选中该项；右键不改变非批量模式的可见状态。
- 测试：`FileList`（`modifier-select`/`context-menu` 事件与坐标）与 `FileBrowser`
  （Ctrl 加选 + Shift 范围得到 3 项、右键菜单执行删除动作、外部点击关闭后可再次打开）
  新增用例。

### 2.10 过期响应丢弃（round 6，稳定性）

- 审计发现两处竞态：快速切换目录或连续搜索时，先发出的请求可能后返回，用旧数据
  覆盖新目录/新搜索结果；同时共享的 `loading` 会被过早置否。
- `files.store` 的 `loadFiles` 与 `FileBrowser.doSearch` 引入递增请求序号：只有最新
  一次请求可以写入状态并结束 loading，过期响应直接丢弃；`clearSearch` 也会使在途
  搜索失效，避免清空后旧结果回填。
- 测试：`files.store`（旧目录响应被丢弃、loading 保持到最新请求结束）与
  `FileBrowser`（旧搜索响应不覆盖新结果）新增用例。

### 2.11 缩略图缓存治理与可观测性（round 6，稳定性）

- 缩略图缓存此前无上限，孤儿缩略图（源文件已删除）会长期占用磁盘。
- 每写入 64 次触发一次后台清理：按 mtime 从旧到新回收，直到条目数不超过 2000（目标
  1600）；孤儿条目与失败残留的 `.tmp` 会自然变旧被回收。
- 增加日志：生成/命中为 `debug`，清理为 `info`（含回收数量与字节）。
- 测试：清理到目标数量且保留最新条目、低于上限时不做任何删除。

### 2.12 拆分 FileBrowser：下载队列与文件预览（round 7，稳定性）

- 新增 `composables/useDownloadQueue.ts`：串行下载、进度、取消、失败提示与队列面板
  状态整体抽出；文件名推导与进度文案复用共享的 `formatSize`。
- 新增 `composables/useFilePreview.ts`：预览状态与 `openPreview`/`closePreview`，
  以及扩展名→预览类型、MIME 推断、HTML 转义与链接/图片来源白名单等纯函数；
  markdown/highlight 仍是动态 import，不进入首屏包。
- `FileBrowser.vue` 由 3288 行降至 2858 行（−430 行，−13%），主组件进一步收敛为
  编排层。
- 测试：下载队列串行执行、排队取消、清理已完成、进度文案；预览类型识别、MIME
  推断与 XSS 白名单（`javascript:`/`data:text/html` 被拒绝）。

### 2.13 继续拆分 FileBrowser：搜索与下载面板（round 8，稳定性）

- 新增 `composables/useFileSearch.ts`：查询条件、结果、加载态、桌面高级搜索面板与
  搜索历史（含持久化与去重上限），并保留 round 6 的过期响应丢弃逻辑。
- 新增 `components/file-browser/DownloadQueuePanel.vue`：把约 140 行队列模板抽成
  展示型组件（props + 事件），进度文案复用共享的 `formatDownloadProgress`
  （新增于 `utils/filePresentation`）。
- `FileBrowser.vue` 由 2858 行降至 2625 行；相比 round 7 起点累计 −663 行（−20%）。
- 测试：搜索历史去重/上限/持久化、作用域路径、`clearSearch` 状态复位；下载面板
  渲染状态标签与进度、面板按钮与单项取消/移除事件、折叠与空状态。

### 2.14 拆分 FileBrowser：目录管理与路径工具（round 9，稳定性）

- 新增 `utils/filePaths.ts`：`isSafeDirName`、`buildChildPath`/`buildSiblingPath`、
  `parentDirectoryPath`、`normalizeTargetDirectory`、`resolveMoveTargetPath`、
  `planMoveOperations` 等纯函数，便于复用与测试。
- 新增 `composables/useDirectoryManager.ts`：目录对话框状态与新建/重命名/删除当前
  目录、重命名条目、变更后刷新；依赖（`navigateTo`/`refresh`/`doSearch` 等）通过
  参数注入，便于测试。
- `FileBrowser.vue` 由 2625 行降至 2430 行；相比 round 7 起点累计 −858 行（−26%）。
- 测试：路径构造/规整、移动目标的自身子目录与重名校验；目录名跟随、新建子目录后
  刷新（含搜索态重新搜索）、重命名跳转、删除需输入匹配目录名、跨目录新建后跳转。

### 2.15 拆分 FileBrowser：批量选择与批量操作（round 10，稳定性）

- 新增 `composables/useFileSelection.ts`：选择状态、Ctrl/⌘ 加选与 Shift 范围选择、
  全选/半选、批量下载/删除/移动/重命名；视图数据与导航依赖通过 `deps` 注入。
- `handleContextMenu` 复用 `narrowSelectionTo`，行为不变。
- `FileBrowser.vue` 由 2430 行降至 2268 行；相比 round 7 起点累计 −1020 行（−31%）。
- 测试：单项切换与退出批量清空、Ctrl+Shift 范围、全选/再点清空、批量删除刷新
  （含搜索态重搜）、取消确认后不删除、批量移动使用当前目录、仅选中一项才重命名。

### 2.16 可点击面包屑与子文件夹跳转（round 11，交互）

- 审计发现 `Breadcrumb.vue` 是**未被引用的死组件**，而实际路径栏只是一段不可点击的
  文本；这与主流云盘“点路径段回跳/展开子目录”的体验不符。
- 重写并接入 `Breadcrumb.vue`：根目录带 home 图标，各段可点击跳转；末段提供下拉，
  列出当前目录的子文件夹可直接进入；搜索态下跳转先退出搜索；支持横向滚动、
  点击外部/`Esc` 关闭。
- `FileBrowser.vue` 用组件替换原纯文本路径（同时移除 `currentPathLabel` 与旧样式）。
- 测试：段落渲染与点击事件、子目录下拉展开/选择后关闭、无子目录不显示触发器、
  点击外部关闭。

### 2.17 静态资源构建期预压缩（round 12，性能）

- 新增 `client/scripts/precompress.mjs`：`bun run build` 在 `vite build` 之后为
  dist 中 ≥1KB 的文本资源生成 `.br`（quality 11）与 `.gz`（level 9），仅在更小时落盘。
- `frontend.rs` 读取 `Accept-Encoding`（支持 `*` 通配与 `q=0`，优先 br），命中
  `<asset>.br`/`<asset>.gz` 时直接返回并带上 `Content-Encoding` 与 `Vary`；
  否则回退原文由中间件实时压缩。
- 显式声明 `Content-Length`：预压缩响应不再退化为 chunked，实测 CSS
  `content-length: 28722` 与磁盘 `.br` 完全一致（brotli q11 优于原 gzip 41.6KB）。
- 测试：`Accept-Encoding` 解析（通配/q=0/q>0）、brotli 优先；集成测试覆盖
  br/gzip/无编码三种路径与 `Vary` 头。

### 2.18 失败请求的有限重试与手动重试（round 13，稳定性）

- 审计发现：网络抖动或 502/503/504 时，幂等 GET 直接失败；列表加载失败后除通知外
  没有任何恢复入口。
- `api.service.ts` 增加响应拦截器的有限重试：仅 `GET`、仅网络/超时错误或
  `429/502/503/504`，最多 2 次，指数退避（300/600ms）加 0–100ms 抖动；调用方主动
  取消（`signal.aborted` / `ERR_CANCELED`）不重试。非幂等请求始终不自动重试。
- 文件列表的错误态新增「重试」按钮（桌面与移动），直接复用现有 `refresh()`。
- 测试：重试判定（GET 网络错误/超时/可重试状态码、POST 与取消不重试）、退避与上限、
  以及加载失败→点击重试→重新拉取成功的集成用例。

### 2.19 预览内的上一个/下一个导航（round 14，交互）

- 对标主流云盘的图片/文档查看器：预览支持在当前视图内连续浏览。
- `useFilePreview` 新增可选依赖 `getPreviewableFiles`，派生 `previewIndex`/
  `previewTotal`/`canGoPrev`/`canGoNext` 与 `prevPreview()`/`nextPreview()`；
  切换复用 `openPreview`，会正确释放上一个 objectURL。
- 预览弹窗新增导航栏（上一张 / `n / total` / 下一张），仅在可预览文件 >1 时显示；
  键盘 ←/→ 在预览打开时切换（不影响其他快捷键）。
- 测试：导航边界（首/末禁用、越界不动、单文件不可导航）与 FileBrowser 集成
  （双击打开 → → 下一张 → ← 上一张，标题随之更新）。

## 3. 后续迭代计划（按优先级）

### 3.1 静态资源预压缩（性能，高）

- `[x]` 构建期生成 `.br`/`.gz`，服务端按 `Accept-Encoding` 直接返回（round 12）。
- `[ ]` 可选：对二进制资源也做预压缩评估；为 `index.html` 等小文件决定是否降低阈值。

### 3.2 缩略图格式与容量（性能 + 稳定性，中）

- `[x]` 容量上限 + 按 mtime 回收，日志可观测（round 6）。
- `[ ]` 支持 `AVIF`/`TIFF` 等更多格式，或按需返回 WebP；按总字节数（而非条目数）设限。

### 3.3 `FileBrowser.vue` 拆分（稳定性，中）

- `[x]` 下载队列与文件预览抽为 composable（round 7，3288 → 2858 行）。
- `[x]` 搜索抽为 `useFileSearch`，下载队列面板抽为 `DownloadQueuePanel.vue`
  （round 8，2858 → 2625 行）。
- `[x]` 目录管理与移动/路径计算抽为 `useDirectoryManager` + `utils/filePaths`
  （round 9，2625 → 2430 行）。
- `[x]` 批量选择/批量操作抽为 `useFileSelection`（round 10，2430 → 2268 行）。
- `[ ]` 继续抽出预览/分享/历史等对话框编排与桌面/移动工具栏模板，目标 < 1500 行。
- 验收：单文件行数持续下降，已有测试保持通过并补充拆分后的单元测试。

### 3.4 交互增强（中，对齐主流云盘）

- `[x]` 列表视图点击表头排序、`Ctrl/⌘+A`、`Delete`/`F2`/`Enter`/`Esc`（round 4）。
- `[x]` 右键上下文菜单；Shift 范围选择 / Ctrl(⌘) 加选（round 5）。
- `[x]` 面包屑可点击跳转 + 当前目录子文件夹下拉（round 11）。
- `[x]` 预览内上一个/下一个（按钮 + ←/→ 方向键）与位置指示（round 14）。
- `[ ]` 面包屑拖放移动；网格视图排序入口与列表一致；移动端长按呼出菜单。
- `[ ]` 上传/下载与网格视图的空状态、加载骨架屏统一。

### 3.5 稳定性与可观测性（中）

- `[x]` 前端目录/搜索过期响应丢弃；缩略图命中/生成/清理日志（round 6）。
- `[x]` 幂等 GET 的有限自动重试 + 列表加载失败重试按钮（round 13）。
- `[ ]` 为内容/下载等 `fetch` 路径补充重试或可恢复失败提示。
- `[ ]` 图片解码失败、超大文件跳过等场景补充计数指标。

### 3.6 暗色主题（交互，中）

- 若要正式支持暗色主题，需要同时提供 Bulma `themes` 变量与自定义样式（Home、
  FileCard、FileItem 等硬编码颜色）的暗色分支，并提供主题切换与持久化。
