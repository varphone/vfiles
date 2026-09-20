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

### 验证基线（round 31 实测）

- `cargo test --workspace`：通过。
- `cargo clippy --workspace --all-targets`：无告警。
- `client` 单测：23 个文件 / **134** 个用例通过；`vue-tsc`、`eslint` 通过。
- 冒烟（Playwright 真实浏览器）：浅色/深色/跟随系统三种模式渲染正确（body 背景
  浅色 `rgb(255,255,255)` / 深色 `rgb(20,22,26)`），界面上的主题切换会写入
  `localStorage` 并即时生效，列表、网格、预览弹窗、移动对话框均无控制台报错；
  服务 `SIGTERM` 优雅退出。

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
| 交互   | 只有浅色主题，深色系统下出现“深色组件 + 浅色面板”         | 深色系统用户观感异常       | `[x]` 见 §2.36 |

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
  暗色主题作为独立事项列入 §3.6，已在 round 31 完成（§2.36）。
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

### 2.20 拆分 FileBrowser：移动对话框（round 15，稳定性）

- 新增 `composables/useMoveDialog.ts`：移动对话框状态与批量移动执行（目标目录规整、
  重名校验、逐项移动后同步选择与高亮、成功/失败提示、完成后刷新）。
- 依赖（当前目录、刷新、选择同步、高亮读写）通过 `deps` 注入；选择相关 helper 用
  延迟箭头调用，避免与 `useFileSelection` 的初始化顺序耦合。
- 顺带清理 `FileBrowser.vue` 中不再使用的 `filesService` 与路径工具导入。
- 测试：初始路径规整、单条目默认父目录、提交中禁止关闭、批量移动与刷新、重名冲突
  保持对话框打开、移出当前目录时清除高亮。

### 2.21 大目录分批渲染推广到桌面端（round 16，性能）

- 审计发现：移动端已有分批渲染（首批 40 / 每次 30），但桌面端一次性渲染全部行/卡片；
  数千条的目录会产生大量 DOM 节点并拖慢滚动。
- 将分批渲染统一到两种布局：`visibleCount` 作为共享状态，`desktopItems` 与移动端列表
  一样按页切片；桌面列表/网格末尾新增哨兵与「已显示 n / total」提示。
- 重写补齐逻辑为 `maybeLoadMore()`：只要哨兵仍在视口附近就继续补齐（带 120px 预取
  边距），避免 IntersectionObserver 在同一交叉状态不再回调导致“卡在首批”。
- 测试：45 项目录仅渲染首批 40 行、显示分批提示且不含末项；既有用例全部保持通过。

### 2.22 拆分 FileBrowser：移动端手势（round 17，稳定性）

- 新增 `composables/useTouchGestures.ts`：下拉刷新（阈值 60 / 上限 90）与左缘右滑返回
  （边缘 24px、水平位移 >80 且垂直 <60），含指示器显隐与刷新成功提示。
- `FileBrowser` 只保留模板绑定，传入 `enabled`（移动端）、`isBlocked`、`refresh`、
  `goBack`；顺带统一为 `anyOverlayOpen`，目录管理弹窗打开时也不再触发手势。
- `FileBrowser.vue` 由 2331 行降至 2249 行；相比 round 7 起点累计 −1039 行（−32%）。
- 测试：达到阈值触发刷新、短距离不触发、左缘右滑返回、非边缘不返回、禁用/被弹窗
  阻塞时完全不响应。

### 2.23 目录列举的 N+1 查询修复（round 18，性能）

- 审计后端时发现 `live_tree` 对每个子项都调用一次 `version_for_entry`（`find_version`），
  即一个目录 N 个文件会产生 N 次额外查询。
- `EntryRepo` 新增 `find_versions(&[VersionId])`：SQLite 侧用 `QueryBuilder` 单条
  `IN (...)` 查询（按 500 分批以规避绑定参数上限），缺失 id 自动跳过。
- `live_tree` 先收集文件类子项的 `current_version_id`，一次批量取回后以
  `HashMap<VersionId, EntryVersion>` 组装，查询次数由 N+1 降为 2。
- 测试：批量查询返回全部版本、空输入返回空、缺失 id 被跳过；既有 HTTP 集成测试
  （含目录列举）保持通过。

### 2.24 快照收集与命名空间列举的查询优化（round 19，性能）

- 继续后端审计发现两处放大效应：
  1. `collect_snapshot_state`（每次写操作都会执行）遍历命名空间全部条目，并对**每个
     条目**调用一次 `find_version`；
  2. `collect_namespace_entries` 通过按目录递归 `find_children` 列举，查询次数为
     O(目录数)。
- `EntryRepo` 新增 `find_all(namespace_id)`：单条 SQL（含 `current_version_id`
  相关子查询）取回全部条目并按路径排序；`collect_namespace_entries` 改为直接调用它。
- `collect_snapshot_state` 先收集文件类条目的 `current_version_id`，一次
  `find_versions` 批量取回后用 HashMap 组装；`pending_snapshot_entry` 不再需要
  每个条目一次查询。
- 结果：写操作从「O(目录数) + O(文件数) 次查询」降为固定 2 次查询。
- 测试：`find_all` 返回全部条目、按路径排序、文件带当前版本而目录为空；既有
  快照/历史/上传等 59 个 HTTP 集成测试保持通过。

### 2.25 子树遍历改为单次范围查询（round 20，性能）

- 审计发现 `collect_descendants`（删除目录、历史/差异按目录作用域、移动目录）按目录
  递归 `find_children`，查询次数为 O(目录数)。
- `EntryRepo` 新增 `find_subtree(namespace_id, root_path)`：用范围比较
  `path = root OR (path >= "root/" AND path < "root0")` 一次取回 root 及全部后代，
  可命中 `(namespace_id, path)` 索引；相比 `LIKE "root/%"` 无需转义、且不受
  `case_sensitive_like` 影响。
- `collect_descendants` 改为调用 `find_subtree`，删除目录由 O(目录数) 次查询降为 1 次。
- 测试：`find_subtree("docs")` 只返回该分支，`docs2` 等同前缀兄弟目录不被误伤；既有
  删除/移动/历史等 59 个 HTTP 集成测试保持通过。

### 2.26 递归删除的批量版本汇总与批量删除（round 21，性能）

- 审计删除路径发现仍有逐条查询：每个被删条目一次当前版本查询、一次完整历史查询、
  一次删除、一次 blob 引用释放。
- 删除类型的快照项会忽略版本字段，因此当前版本查询是多余的，直接去掉。
- `EntryRepo` 新增：
  - `find_versions_for_entries(&[EntryId])`：一次取回多条条目的全部版本（按 500 分批），
    用于汇总 blob 引用；
  - `delete_entries(&[EntryId])`：`DELETE ... WHERE id IN (...)` 批量删除。
- 删除流程改为：批量汇总历史 → 汇总 blob 引用并聚合去重 → 批量删除条目 →
  一次性 `release_blob_references` → 删除归零 blob。61 条目 / 120 版本的子树删除约
  12ms（此前约 4×N 次查询，现为常数级）。
- 测试：批量历史覆盖所有条目、空输入返回空、批量删除后 `find_all` 为空；既有
  删除/移动/历史/快照等 59 个 HTTP 集成测试保持通过。

### 2.27 移动路径的批量校验与事务更新（round 22，性能）

- 移动目录时会遍历全部后代，旧实现对每个新路径调用一次 `find_by_path` 做冲突检查
  （O(N) 次查询），并在循环中逐条 `move_entry`（无事务、逐条提交）；同时
  `original_paths` 用 Vec 线性查找，整体 O(N²)。
- `EntryRepo` 新增：
  - `find_paths(namespace_id, &[NormalizedPath])`：一次批量查询存在的路径（按 500 分批）；
  - `move_entries(&[(EntryId, NormalizedPath)])`：单事务内批量更新，并保留唯一约束冲突到
    `PathConflict` 的映射。
- `move_entries`（app）改用 `HashSet` 做 O(1) 的「是否原路径」判断、一次批量冲突检查、
  一次事务批量更新。
- 测试：`find_paths` 只返回存在的路径、空输入为空；`move_entries` 批量成功、重复目标
  返回 `PathConflict`；既有移动/重命名等 59 个 HTTP 集成测试保持通过。

### 2.28 拖放移动（round 23，交互）

- 列表行与网格卡片支持 HTML5 拖拽：真实条目可拖动（`dragstart` 写入
  `dataTransfer`），目录作为放置目标（`dragover`/`dragleave`/`drop`），并有
  `.drop-target` 高亮（App.vue 全局样式）。
- 面包屑路径段同样可作为放置目标，把条目移动到任意祖先目录。
- `useMoveDialog` 抽出 `performMove` 核心，新增 `moveEntryToDirectory`；拖放复用同一
  套校验（自身/子目录、未变化、重名）与刷新逻辑，不打开对话框。
- 测试：`FileList` 拖拽事件与目录放置、非目录忽略放置；`Breadcrumb` 放置事件；
  `useMoveDialog` 拖放移动/拒绝移动到自身子目录；`FileBrowser` 拖拽文件到目录行后
  调用 `movePath`。

### 2.29 列表/网格加载骨架屏（round 24，交互）

- 原先加载态只有一个 spinner + 「加载中...」；主流云盘使用骨架屏减少布局跳动并
  提示内容形态。
- 新增 `FileSkeleton.vue`：`variant` 支持 `list`（行：图标 + 名称 + 日期 + 大小）与
  `grid`（缩略图卡片），默认 6/8 项，带微光动画，并遵循
  `prefers-reduced-motion`；容器为 `role="status" aria-busy="true"` 且含
  `sr-only` 文本。
- `FileBrowser` 桌面与移动端的加载态改为按当前视图渲染对应骨架屏（预览弹窗的
  spinner 保留）。
- 测试：骨架屏行/卡片数量、默认 6 行、`aria-busy` 与可读文本；FileBrowser 加载中
  展示骨架屏并在完成后渲染空目录状态。

### 2.30 服务端优雅停机（round 25，稳定性）

- 审计发现 `axum::serve(...).await` 没有优雅停机：systemd 重启/停止或 Ctrl+C 会直接
  中断进程（退出码 143），在途请求被截断、连接池未显式关闭。
- 新增 `shutdown_signal()`：同时等待 `SIGINT`（Ctrl+C）与 `SIGTERM`（Unix），
  通过 `with_graceful_shutdown` 先停止接收新请求并等待在途请求结束，随后关闭
  SQLite 连接池并输出 `VFiles server stopped`。
- 文档：`docs/DEPLOYMENT.md` 的 systemd 章节补充优雅停机行为说明。
- 验证：启动服务 → `/api/health` 200 → `kill -TERM` → 日志依次出现
  `Shutdown signal received`、`closing database pool`、`VFiles server stopped`，
  进程**退出码 0**。

### 2.31 失败预览与下载的重试入口（round 26，稳定性）

- 审计发现：列表加载失败已有「重试」，但**预览失败**与**下载失败/取消**只能重新走一遍
  操作，缺少就地重试。
- `useDownloadQueue` 新增 `retryItem(id)`：把失败/已取消条目重置为排队（清空进度与错误）
  并继续处理；`DownloadQueuePanel` 对 `error`/`canceled` 条目显示「重试」按钮。
- 预览错误状态新增「重试」按钮，复用 `openPreview(preview.path)`。
- 顺带修复测试隔离：`FileBrowser` 测试的 `beforeEach`（mock 重置 + `matchMedia` 打桩）
  提升到模块级，使所有 `describe` 与 `-t` 单跑都具备一致的浏览器环境。
- 测试：下载失败后重试成功、活动条目不重复触发、面板按钮显隐与事件、预览失败后重试
  再次请求内容。

### 2.32 孤儿 blob 文件回收（round 27，稳定性）

- 审计发现：`store_blob` 只写磁盘文件，元数据行由 `create_version` 建立；若上传
  在两者之间失败，会在 blob 目录留下**有文件、无元数据行、无任何引用**的孤儿文件，
  现有删除路径只处理「有行」的 blob，无法回收它们。
- `BlobStore` 新增：
  - `list_stored_blob_files()`：按 `<前2位>/<其余>` 目录结构还原 `BlobId` 并返回
    文件修改时间与大小；
  - `purge_blob(blob_id, expected_ref_count)`：乐观校验后删除「行 + 文件」；
  - `list_blobs()` 供行级扫描。
- `EntryRepo::referenced_blob_ids()`：`UNION` 版本表与快照表的 blob 引用集合。
- 新增 `MaintenanceService::purge_orphan_blobs(grace_seconds)`：先做行级清理（无引用
  且超过保护期），再删除磁盘上无元数据行的孤儿文件；保护期避免误删上传中的文件。
- CLI：`vfiles maintenance gc-blobs [--grace-seconds N]`（默认 3600s），输出清理数量与
  释放字节；文档见 `docs/DEPLOYMENT.md`。
- 测试：`is_purgeable` 判定；真实 SQLite + 文件系统下清理孤儿文件、保留被引用与
  保护期内的文件；冒烟验证 CLI 与 `--grace-seconds` 行为。

### 2.33 快照保留策略（round 28，稳定性）

- 观察：`add_snapshot_entries` 会为快照中每个 blob 增加 `ref_count`，而删除只按版本
  引用递减，因此只要仍有历史快照，blob 就不会归零——历史快照会持续占用磁盘。
- `SnapshotRepo` 新增 `list_all_snapshots()`（按创建时间倒序）与 `delete_snapshot()`
  （`snapshot_entries` 通过外键级联删除）。
- `MaintenanceService` 扩展为泛型 `<BlobStore, EntryRepo, SnapshotRepo>`，新增
  `prune_snapshots(keep)`：按命名空间分组、每组保留最新 `keep` 个快照，删除其余快照并
  汇总其 blob 引用、调用 `release_blob_references` 释放（归零行删除、文件由
  `delete_blob` 清理）。
- CLI：`vfiles maintenance prune-snapshots --keep N [--grace-seconds N]`，执行后自动
  运行一次孤儿 blob 回收；文档见 `docs/DEPLOYMENT.md`。
- 测试：真实 SQLite 下 3 个快照（各引用同一 blob）裁剪 `keep=1` 后仅剩 1 个快照，
  且 blob `ref_count` 恰好减少被删快照的引用数；冒烟验证 CLI 行为。

### 2.34 移动端长按呼出菜单（round 29，交互）

- 背景：桌面右键与 Android Chrome 长按会触发 `contextmenu`，但 iOS Safari 不可靠，
  移动端缺少统一的长按菜单入口。
- `FileItem`（移动端列表）与 `FileCard`（网格）新增 500ms 长按识别：以触摸点坐标发出
  与右键一致的 `context-menu` 事件，复用现有 `ContextMenu` 组件；手指移动 / 抬起 /
  取消即中止，且长按后紧随的 `click` 会被忽略，避免误触选择或打开。
- 测试：长按发出正确坐标、移动取消不触发、长按后 click 被抑制；既有 119 个前端用例
  保持通过。

### 2.35 服务端目录分页（round 30，性能）

- 背景：`GET /api/files/tree/{path}` 一次性返回目录全部子项，客户端虽已分批渲染，
  但超大目录的 JSON 体积与服务端查询/序列化成本仍在（见 §3.1b）。
- 新增只读接口 `GET /api/files/list` 与 `GET /api/files/list/{*path}`，查询参数
  `limit`/`offset`/`commit`；`limit` 夹取到 `1..=1000`（默认 200），响应为
  `EntryPageDto { items, total, limit, offset, has_more }`（`has_more` 由
  `offset + items.len() < total` 计算，避免客户端自行推断）。
- 分页与既有 `/tree` 接口共用同一套命名空间/提交快照读取路径（`fetch_entry_items`），
  保持权限、`commit` 历史浏览与排序语义完全一致；`/tree` 保持不变以兼容
  `getFiles` 的既有调用方（如移动对话框的重名检查）。
- 前端：`files.service.getFilesPage()`；`files.store` 新增 `totalFiles` /
  `hasMoreFiles` / `loadingMoreFiles` 与 `loadMoreFiles()`（沿用 `loadSequence`
  序号守卫，目录切换后到达的旧页会被丢弃）；`FileBrowser` 的哨兵在本地分片渲染
  完毕后按需拉取下一页，状态栏显示「已加载 / 总数」，加载中提示「正在加载更多...」。
- 稳定性：追加分页失败单独记录在 `loadMoreError`（不清空已加载条目、不整屏报错），
  哨兵处给出「重试」入口。
- 测试：后端集成测试 `paginated_directory_listing_reports_total_and_pages`；
  前端新增 4 个 store 用例（首页字段与追加、目录切换后旧页丢弃、无更多时不发请求、
  追加失败保留已加载数据）与 1 个组件用例（分页进度展示），共 **124** 个用例通过。

### 2.36 暗色主题（round 31，交互）

- 背景：此前 `bulma.scss` 固定为浅色（注释里已标记“将来要支持暗色主题”），
  组件内散落约 200 处硬编码色值，深色系统下会出现“深色组件 + 浅色面板”。
- 新增 `src/styles/theme.scss` 设计令牌层：浅色/深色两套 `--vf-*` 语义变量
  （画布、面板、边框、文本、强调、语义色、阴影、骨架屏、滚动条），共 53 个，
  全部有实际使用点；`bulma.scss` 改为 `@use "bulma/sass/themes"`，Bulma 组件
  自动跟随 `data-theme` 或系统偏好。
- 主题切换：`theme.store.ts`（`system`/`light`/`dark`，localStorage 持久化、
  监听 `prefers-color-scheme` 变化）+ `ThemeToggle.vue` 下拉菜单，接入首页导航栏
  与登录页；`index.html` 内联脚本在首屏绘制前应用已保存主题，避免白屏闪烁，
  并同步 `meta[name=theme-color]`。
- 迁移范围：文件列表/网格/面包屑/右键菜单/骨架屏/预览/移动对话框/分享对话框/
  版本历史/上传区域/首页与登录页等 18 个组件，约 200 处色值改为令牌；
  `FileItem.vue` 里遗留的 `@media (prefers-color-scheme: dark)` 块被令牌取代
  （它此前不响应显式切换）。
- 成本：index CSS 546 KB → brotli **30.1 KB**（新增深色变量后 br 仅 +3.1 KB，
  全量 br 176.7 KB → 179.8 KB），首屏传输基本不变。
- 测试：新增 7 个 store 用例（默认跟随系统、显式浅色覆盖系统深色、持久化与清除、
  读取已保存值、忽略非法值、仅在跟随系统时响应系统切换、循环切换）与 3 个组件
  用例（切换后写入 localStorage 与 `data-theme`、当前项 `aria-checked`、共享 store），
  合计 134 个用例。

### 2.37 关闭认证时前端仍要求登录（round 31，稳定性）

- 问题：`/api/auth/me` 的 `enabled` 与 `session/bootstrap` 的 `auth_enabled`
  取自“认证服务是否存在”（实际总是存在），而接口鉴权 `protected_request_context`
  取的是 `config.auth.enabled`。于是 `VFILES_AUTH_ENABLED=false` 时 API 无需登录，
  前端却仍被路由守卫重定向到登录页，且登录页的“未启用认证”提示永不出现。
- 修复：两处统一以 `state.config.auth.enabled` 为准（`crates/vfiles-http/src/routes/
  auth.rs`、`session.rs`），与鉴权判断保持一致。
- 验证：真实服务下 `curl /api/auth/me` 返回 `enabled:false`、bootstrap
  `auth_enabled:false`，浏览器直接进入文件浏览器且无报错。

## 3. 后续迭代计划（按优先级）

### 3.1 静态资源预压缩（性能，高）

- `[x]` 构建期生成 `.br`/`.gz`，服务端按 `Accept-Encoding` 直接返回（round 12）。
- `[ ]` 可选：对二进制资源也做预压缩评估；为 `index.html` 等小文件决定是否降低阈值。

### 3.1c 删除/移动路径的子树遍历（性能，中）

- `[x]` `EntryRepo::find_subtree` 单次范围查询取代按目录递归（round 20）。
- `[ ]` 移动时的冲突检查仍对每个后代调用一次 `find_by_path`；可增加批量路径存在性
  查询。

### 3.1b 服务端目录分页（性能，高）

- `[x]` 新增 `GET /api/files/list[/{path}]`，支持 `limit`/`offset`/`commit` 与
  `total`/`has_more`，客户端按需加载（round 30）。
- `[ ]` 搜索结果分页（当前 `/api/files/search` 仍一次性返回）。
- `[ ]` 用游标（cursor）替代 `offset`，避免大目录下深分页的 `OFFSET` 扫描成本。

### 3.1e 移动路径的批量校验与事务（性能，中）

- `[x]` 批量路径存在性检查 + 事务内批量更新（round 22）。

### 3.1d 快照保留与 blob 回收（稳定性，中）

- `[x]` 孤儿 blob 文件 GC（round 27）：清理「有文件、无元数据行、无引用」的残留。
- `[x]` 快照保留策略（round 28）：`prune-snapshots --keep N` 裁剪旧快照并释放其
  blob 引用，配合 `gc-blobs` 回收磁盘。
- `[ ]` 可按时间窗口（而非数量）保留快照；在服务内按周期自动执行维护任务。

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
- `[x]` 移动对话框抽为 `useMoveDialog`（round 15，2362 → 2313 行）。
- `[x]` 移动端手势抽为 `useTouchGestures`（round 17，2331 → 2249 行）。
- `[ ]` 继续抽出分享/历史/上传器等对话框编排与桌面/移动工具栏模板，目标 < 1500 行。
- 验收：单文件行数持续下降，已有测试保持通过并补充拆分后的单元测试。

### 3.4 交互增强（中，对齐主流云盘）

- `[x]` 列表视图点击表头排序、`Ctrl/⌘+A`、`Delete`/`F2`/`Enter`/`Esc`（round 4）。
- `[x]` 右键上下文菜单；Shift 范围选择 / Ctrl(⌘) 加选（round 5）。
- `[x]` 面包屑可点击跳转 + 当前目录子文件夹下拉（round 11）。
- `[x]` 预览内上一个/下一个（按钮 + ←/→ 方向键）与位置指示（round 14）。
- `[x]` 拖放移动：拖到目录行/卡片或面包屑路径段（round 23）。
- `[x]` 加载骨架屏（列表/网格）取代单一 spinner（round 24）。
- `[x]` 移动端长按呼出上下文菜单（round 29）。
- `[ ]` 网格视图排序入口与列表一致。
- `[ ]` 上传/下载与网格视图的空状态、加载骨架屏统一。

### 3.5 稳定性与可观测性（中）

- `[x]` 前端目录/搜索过期响应丢弃；缩略图命中/生成/清理日志（round 6）。
- `[x]` 幂等 GET 的有限自动重试 + 列表加载失败重试按钮（round 13）。
- `[x]` 服务端优雅停机（SIGTERM/SIGINT → 停止收新请求 → 关闭连接池，退出码 0）（round 25）。
- `[x]` 预览/下载失败的「重试」入口与下载队列重试（round 26）。
- `[ ]` 为内容/下载等 `fetch` 请求本身补充有限自动重试（当前仅手动重试）。
- `[ ]` 图片解码失败、超大文件跳过等场景补充计数指标。

### 3.6 暗色主题（交互，中）

- `[x]` 设计令牌 + Bulma 双主题 + 切换入口 + 全组件迁移（round 31）。
- `[ ]` 代码预览缺少语法高亮配色（`highlight.js` 只加载了语言包，没有引入
  任何主题 CSS），可在标记层补一套浅色/深色 hljs 配色。
- `[ ]` 深色下部分按钮仍使用 `is-light`（浅底），可统一为主流云盘的“幽灵按钮”
  风格。

- 若要正式支持暗色主题，需要同时提供 Bulma `themes` 变量与自定义样式（Home、
  FileCard、FileItem 等硬编码颜色）的暗色分支，并提供主题切换与持久化。
