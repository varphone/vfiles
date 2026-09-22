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

### 验证基线（round 63 实测·本轮迭代收尾）

- `cargo test --workspace`：通过。
- `cargo clippy --workspace --all-targets`：无告警。
- `client` 单测：42 个文件 / **265** 个用例通过；`vue-tsc`、`eslint`、`prettier`
  通过。
- 后端：`cargo test --workspace` 全部通过、`clippy --all-targets` 无告警、`fmt`
  干净（`frontend.rs` 的历史格式差异保持原样）。
- 缩略图冒烟：70 张 PNG 全部返回 `200 image/jpeg`；把字节上限设为 0 时日志出现
  `pruned thumbnail cache removed=64 removed_bytes=47424 remaining_entries=1`（证明按
  字节回收与「至少保留最新一条」生效）；手工构造的 TIFF 与 ICO 也返回
  `200 image/jpeg`（`ffd8ffe0` 开头），无生成失败日志。
- 界面：Playwright 对浅色/深色桌面、网格视图、批量模式、移动端与移动对话框逐张截图
  核对（见 §2.41、§2.42、§2.43）；右侧详细信息面板的初始状态、跟随选中、显示/隐藏
  持久化、移动端不渲染均有断言覆盖；顶栏版本胶囊、移动端单行底栏与移动搜索流程
  已在真实浏览器验证。
- 冒烟（真实服务）：开启周期性维护后 1 秒执行首轮并输出
  `Periodic maintenance finished pruned_snapshots=0 released_blobs=0 purged_blobs=1 freed_bytes=14`，
  伪造的孤儿 blob 文件被实际删除；**默认（未开启）时孤儿文件保留**，证明维护是显式开关；
  `SIGTERM` 时先打印 `Periodic maintenance stopped` 再 `VFiles server stopped`，优雅退出。

### 主要发现

| 类别   | 问题                                                      | 影响                       | 状态          |
| ------ | --------------------------------------------------------- | -------------------------- | ------------- |
| 性能   | 预览按需加载完整 `highlight.js`（约 190 种语言），914KB   | 首次代码预览加载缓慢       | `[x]` 见 §2.1 |
| 性能   | 全量 Bulma CSS 约 678KB，渲染阻塞                         | 首屏 CSS 体积大            | `[x]` 见 §2.6 |
| 性能   | 静态前端资源完全未压缩，首屏 CSS/JS 明文传输              | 首屏传输体积是压缩后的 10x | `[x]` 见 §2.7 |
| 性能   | 缺少服务端缩略图接口，网格视图直接拉取原图                | 大图目录流量偏高           | `[x]` 见 §2.5 |
| 交互   | 文件列表不支持排序                                        | 与主流云盘差异明显         | `[x]` 见 §2.2 |
| 交互   | 仅有表格视图，无网格/缩略图视图                           | 图片目录浏览体验差         | `[x]` 见 §2.3 |
| 稳定性 | `FileBrowser.vue` 单文件 2788 行（已拆到 2031），职责过载  | 维护与回归风险             | `[~]` 见 §3.3 |
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

### 2.38 代码预览语法高亮与一键复制（round 32，交互）

- 背景：`highlight.js` 此前只加载了语言包、**没有任何配色**，代码与 Markdown
  代码块都是单色文本；深色主题上线后这个短板更明显。
- 新增 `src/styles/code-theme.scss`：覆盖 highlight.js 的 `hljs-*` 类名（注释、
  关键字、字符串、数字、函数、类型、变量、差异、强调等），配色同样走
  `--vf-*` 令牌（浅色取 GitHub Light、深色取 GitHub Dark 色相），代码块统一
  圆角、内边距与 1px 边框；同时取代 `FileBrowser.vue` 里零散的 `:deep(.hljs-*)`
  规则。
- `highlight.ts` 新增纯函数：`languageForPath()`（扩展名 → hljs 语言名）、
  `highlightCode()`（指定语言失败时回退自动识别）、`renderHighlightedCode()`
  （渲染完整 `<pre><code>`，并过滤围栏语言串里的非法字符防止拼进 class 属性）、
  `hasFencedCodeBlock()`；代码预览改用扩展名推断语言，比自动识别更准。
- Markdown 预览的围栏代码块现在也会高亮：`marked` 渲染器增加 `code` 处理，
  且只有真的出现围栏代码块时才动态加载高亮包（约 150KB），纯文档预览不受影响。
  `marked` 的渲染器是全局合并的，因此只在首次安装一次，避免第二次调用把
  `code` 覆写成不带高亮的版本。
- 一键复制：新增 `src/utils/clipboard.ts`（`copyText()`，Clipboard API 失败时
  回退到临时 `<textarea>` + `execCommand`，并保证临时节点被清理），代码/文本预览
  底部出现「复制」按钮，成功后就地显示「已复制」；`ShareDialog` 改用同一个
  helper，删掉了重复的复制逻辑。
- 测试：新增 10 个 `highlight` 用例（语言推断、围栏识别、回退逻辑、语言串清洗）、
  4 个 `clipboard` 用例（含回退与失败路径，并借此发现并修复了临时节点泄漏）、
  3 个预览渲染用例（Markdown 带/不带围栏、按扩展名高亮的代码预览）与 2 个组件
  用例（复制成功反馈、二进制预览不显示复制按钮）。

### 2.39 原始 fetch 的有界重试与预览取消（round 33，稳定性）

- 背景：axios 实例早就有重试拦截器，但预览内容、版本 diff、带进度的下载走的是
  原始 `fetch`，一次失败就直接报错；预览也没有取消机制，快速切换文件时旧响应
  可能覆盖新内容（与目录列表此前修掉的问题同类）。
- 新增 `src/services/fetch-retry.ts`：复用 axios 的重试策略常量与退避算法
  （`RETRYABLE_STATUS_CODES` 429/502/503/504、`MAX_RETRIES=2`、
  `computeRetryDelayMs` 指数 + 抖动），只重试“拿到响应头之前”的失败——响应体
  开始读取后再重试会让下载进度回退，因此流式失败仍按普通错误处理；重试前调用
  `response.body.cancel()` 释放连接；`signal` 已取消或中途取消时立即停止。
- 接入三处只读原始 `fetch`：`fetchToBlob`（单文件/文件夹下载）、
  `getFileContent`（预览）、`getFileDiff`（版本 diff）。
- 预览并发治理：`useFilePreview` 增加请求序号 + `AbortController`。切换/关闭预览
  会取消在途请求并让旧响应失效，旧响应不再写回 `preview.value`，也不会误清
  `loading` 或把主动取消当成失败；`getFileContent` 新增可选的 `signal` 参数。
- 测试：新增 8 个 `fetch-retry` 用例（重试后成功、不可重试状态、预算耗尽、网络错误
  重试与抛出、重试前释放响应体、取消后不重试、已取消时直接失败）与 4 个预览并发
  用例（过期响应丢弃、切换时取消在途请求、关闭后不写回、取消不报错）。

### 2.40 周期性维护任务（round 34，稳定性）

- 背景：`gc-blobs` / `prune-snapshots` 此前只能在业务低峰期手动执行（见
  `docs/DEPLOYMENT.md`），长期运行的实例容易积累孤儿 blob 与过多快照。
- 配置：新增 `MaintenanceConfig`（`crates/vfiles-config`），环境变量
  `VFILES_MAINTENANCE_ENABLED`（默认 **false**，因为涉及不可逆删除）、
  `..._INTERVAL_SECONDS`（默认 86400，下限 60）、`..._INITIAL_DELAY_SECONDS`
  （默认 300，且不超过间隔与 5 分钟）、`..._BLOB_GRACE_SECONDS`（默认 3600）、
  `..._SNAPSHOT_KEEP`（默认 0 = 不裁剪快照）。
- `MaintenanceService::run_once()` 把两步合成一次执行：先裁剪快照释放 blob 引用，
  再回收孤儿 blob，返回合并报告（`pruned_snapshots` / `released_blobs` /
  `purged_blobs` / `freed_bytes`）。顺序很重要：先裁剪才能让同一轮回收掉刚释放的 blob。
- `vfiles serve` 在后台启动维护循环：首轮在 `initial_delay` 后立即执行，此后按
  `interval` 执行；单轮失败只告警并在下个周期重试；通过 `tokio::sync::watch` 与
  停机信号联动，收到 SIGTERM/SIGINT 后先停止维护任务再关闭连接池，避免任务持有
  已关闭的连接池。
- 测试：`vfiles-config` 新增默认值（必须显式开启、默认不裁剪快照）与首次延迟收敛
  用例；`vfiles-app` 新增两个 `run_once` 用例（合并报告：裁剪旧快照释放的 blob
  同一轮被回收 + 孤儿文件被清理；`keep=0` 时不裁剪任何快照）；`vfiles-bin` 新增
  调度参数收敛用例。冒烟验证真实服务下的开启/关闭两种行为与优雅停机日志。

### 2.41 界面重设计（第一轮：整体视觉语言与文件列表，round 34，交互）

- 背景：此前的界面是「多层圆角卡片 + 装饰性渐变背景 + 每行常驻 6 个操作图标 +
  整列蓝色文件名」的组合，与主流云盘（Google Drive / OneDrive / Dropbox / 阿里云盘）
  差距明显：视觉噪音大、主次不分、信息密度低。
- 设计语言：
  - 去掉装饰性径向渐变与模糊光斑，改用中性平铺底色（`--vf-canvas`）；
  - 新增圆角（`--vf-radius-*`）、卡片阴影（`--vf-shadow-card`）、应用栏底色
    （`--vf-app-bar`）令牌，统一 6/10/14px 三级圆角；
  - 新增全局控件基类 `src/styles/controls.scss`：`.vf-icon-button`（无边框图标按钮）、
    `.vf-ghost-button`（次要按钮）、`.vf-primary-button`（填充胶囊主操作），
    取代各组件里混杂的 `is-light`/`is-success`/`is-info` 组合。
- 应用外壳：顶栏与移动端底栏改为纯色 + 细线分隔（不再用阴影），内容区改为**单一**
  白色面板（细微阴影 + 细边框），内部区块之间只用 1px 分隔线，不再层层嵌套圆角卡片。
- 工具栏：操作收敛为 图标按钮（上一级 / 刷新 / 视图 / 批量选择）+ 胶囊搜索框 +
  右侧填充主操作「上传」；批量操作条改为横向铺满的强调色条并统一按钮风格。
- 文件列表：去掉斑马纹，粘性表头 + 细分隔线 + 48px 行高，hover/选中态用浅底，
  行内操作按钮改为 hover / 聚焦时才显示；文件名恢复正文色（文件夹加粗），
  目录项 hover 才出现链接感；次要列（时间/类型/大小）改为低对比度小字。
- 时间显示：新增 `formatRelativeDate()`（今天 HH:mm / 昨天 HH:mm / N 天前 / 日期），
  列表与网格统一使用，精确时间放在 `title`。
- 空状态：改为图标 + 标题 + 说明 + 操作按钮（上传文件 / 新建文件夹 / 清空搜索）。
- 网格与移动端：网格卡片统一圆角与 hover 位移；移动端列表由「卡片 + 阴影」改为
  「扁平行 + 细分隔线」，`.`/`..` 快捷项不再整块着色；缩略图加载失败时回退到文件图标
  （冒烟中伪造 PNG 触发 415，界面正确降级为图标，无破图）。
- 品牌一致性：Bulma 的 `primary`/`link` 通过 `derived-variables` 配置为应用强调色
  `#2f6db6`，消除「自定义按钮是蓝色、Bulma 主按钮是青绿色」的割裂。
- 验证：完整前端测试 170 项（新增 5 个相对时间用例）、`vue-tsc`、`eslint`、
  `prettier`、生产构建全部通过；
  Playwright 截图覆盖浅/深色桌面、网格、批量、移动端与对话框，均无控制台报错。

### 2.42 界面重设计（第二轮：详细信息面板）与文件类型判定修正（round 35，交互）

- 新增桌面端右侧「详细信息」面板：图标 + 名称 + 路径、类型/大小/修改时间/创建时间/
  最近提交，以及 2 列快捷操作（打开|预览、下载、分享、历史版本、重命名、移动、删除）。
  面板展示当前活动条目（键盘高亮或唯一选中项），未选中时给出占位提示；工具栏新增
  显示/隐藏开关，开关状态持久化到 `fileView` 存储。
- 顺带修掉两个此前被掩盖的问题：
  - 活动行初始化：目录刚加载时列表里只有 `.`/`..` 两个快捷项，活动行会停在快捷项上，
    导致详情面板长期为空占位；现在真实条目出现后自动改选第一个真实条目（用户已显式
    选择时不打扰）。
  - 文件类型判定：浏览器会把 `.ts` 上报为 `video/mp2t`，此前列表把 TypeScript 源码
    显示成「视频文件」并使用视频图标。`filePresentation` 现在让代码类扩展名优先于
    mime，并删除 `FileItem` 内重复的一份类型判定逻辑。
- 图标统一：抽出 `FileTypeIcon.vue`（列表、网格、详情面板共用），`FileItem` 不再
  自维护一套扩展名→图标映射。
- 测试：新增详细信息面板组件用例 2 个、`fileView` 偏好用例 1 个、文件类型判定用例
  3 个；列表相关断言在测试中显式关闭详情面板以避免文本重复。

### 2.43 界面重设计（第三轮：顶栏与移动端工具栏，round 36，交互）

- 顶栏重设计：
  - 版本/快照切换从「4 个独立按钮 + 状态标签」改为一个紧凑胶囊
    （`« ‹ 当前版本 › »`），当前版本/快照哈希显示在中间，回退后标签实时变为哈希，
    历史有错误时胶囊描边转为危险色；
  - 账号入口改为「圆形头像 + 用户名 + 箭头」的主流样式，未登录时回退到用户图标；
  - 移除移动端汉堡菜单里 5 个与底部操作栏重复的文字链接（刷新/根目录/上一级/
    上传文件/批量模式），菜单只保留账号与主题。
- 移动端工具栏去重：
  - 底部栏原本同时有搜索框和操作行（两行），而内容区工具栏在 ≤1023px 被整体隐藏，
    等于搜索只存在于底栏；现在底栏收敛为**单行**（菜单 + 模式面板），搜索回到内容区
    顶部；
  - 移动端搜索改为紧凑一行：胶囊输入框 + 「筛选/搜索/清空」三个图标按钮，
    全文搜索、类型、仅当前目录折叠在「筛选」按钮后面，不再一屏铺满表单；
  - 底栏与工具栏的图标按钮统一使用 `--vf-radius`、hover 浅底，上传为主操作（填充强调色）。
- 验证：前端 176 项测试、`vue-tsc`、`eslint`、生产构建通过；Playwright 核对顶栏高度
  53px、版本胶囊 4 个箭头 + 中间标签（回退后显示哈希）、账号按钮文本、移动端底栏
  行数为 1 且不再包含搜索框、移动搜索「输入→搜索→命中文档」全流程与筛选面板展开，
  全程无控制台报错。

### 2.44 `FileBrowser.vue` 拆分：详细信息面板与批量操作条（round 37，稳定性）

- 背景：三轮界面改造都落在这个文件上，它一度膨胀到 2788 行，同时承担编排、
  预览、搜索、批量、拖拽、详情面板等职责，继续在其上迭代的回归风险快速上升。
- 抽出 `FileDetailsPanel.vue`（238 行）：详情面板此前是内联的 120 行模板 + 110 行
  样式，现在只接收 `item` 并上抛 8 个事件，不再接触列表/选择状态；
  「详细信息」的计算属性（相对/绝对时间）也随之下沉。
- 抽出 `BatchActionBar.vue`：批量操作条只呈现入口与禁用态，批量逻辑仍留在
  `useFileSelection` 与父组件，避免状态重复。
- 顺带清理随迁移失效的死样式（`.desktop-details*`、`.desktop-batch-*`）。
- 结果：`FileBrowser.vue` 2788 → **2516** 行（-10%），新增两个单职责组件。
- 测试：为两个新组件补充 10 个单元用例（详情面板的占位/元数据/文件与文件夹差异/
  事件上抛/最近提交；批量条的计数/禁用矩阵/事件上抛），前端共 **186** 项通过。
- 验证：`vue-tsc`、`eslint`、`prettier`、生产构建通过；Playwright 回归确认详情面板
  仍显示当前条目（`docs`，7 个操作）、批量条在 0 选中时禁用「下载」、选中 1 项后变为
  「已选 1 项」且按钮可用、详情面板跟随选中变化、代码预览正常，全程无控制台报错。

### 2.45 `FileBrowser.vue` 拆分：预览弹窗（round 38，稳定性）

- 抽出 `FilePreviewModal.vue`（225 行）：预览弹窗约 110 行模板与约 110 行样式
  （spinner/`@keyframes`、图片/PDF/音视频/文本/Markdown/代码各分支、复制与上一张/
  下一张导航）整体迁出。组件只接收 `preview` 状态对象与导航/复制状态，上抛
  `close`/`retry`/`prev`/`next`/`copy`，不再持有业务逻辑。
- `useFilePreview` 导出 `PreviewState` 类型，父组件与弹窗共用同一份状态定义，
  避免弹窗再复制一份字段。
- 顺带清理两处死代码：随迁移失效的预览样式，以及自 round 35 起就没人引用的旧
  详情面板样式（`.desktop-detail-*`，62 行）。
- 结果：`FileBrowser.vue` 2516 → **2277** 行（两轮拆分累计 2788 → 2277，-18%）。
- 测试：新增 8 个弹窗用例（加载/错误与重试上抛、按类型渲染图片与文本、代码高亮
  片段、不支持类型提示、复制按钮可见性与三种反馈、单文件不显示导航、多文件导航
  与禁用态），前端共 **194** 项通过；Playwright 复核代码预览（标题/高亮/「已复制」/
  位置 6-7）、文本预览（仅复制）、Markdown 围栏高亮、方向键导航 1/7 → 2/7，
  全程无控制台报错。

### 2.46 `FileBrowser.vue` 拆分：搜索框与高级筛选（round 39，稳定性）

- 抽出 `BrowserSearchBox.vue`（332 行）：桌面搜索框 + 高级筛选下拉整体迁出，
  包含约 100 行模板、157 行样式，以及此前挂在父组件上的「点击外部关闭」监听。
- 交互通过 v-model 组合暴露：`modelValue`（查询）、`open`（下拉）、`content`
  （全文）、`type`、`scopeCurrent`，外加 `search` / `clear` 事件；输入框通过
  `registerInput` 回调登记到 `useFileSearch`，保留「搜索/清空后自动聚焦」的行为。
  Escape 的层级顺序（高级搜索 → 预览 → 批量 → 选择）仍由父组件统一处理，
  避免拆分后顺序错乱。
- 顺带修正可访问性：类型下拉补充 `aria-label="搜索类型"`（原先只能用 role 定位，
  且与 `datalist` 输入框的 role 冲突）。
- 结果：`FileBrowser.vue` 2277 → **2031** 行（三轮拆分累计 2788 → 2031，-27%）。
- 测试：新增 8 个搜索框用例（占位文案随模式变化、输入/回车/按钮触发搜索、下拉开合、
  筛选变更与清空、全文未启用时禁用、输入框登记与撤销、点击外部关闭），前端共
  **202** 项通过；Playwright 复核搜索（2 条命中）、下拉筛选、点击外部与 Escape
  关闭（结果仍保留）、清空后恢复全量列表，全程无控制台报错。

### 2.47 缩略图缓存按字节设限与更多格式（round 40，性能 + 稳定性）

- 背景：缩略图磁盘缓存此前只按「条目数」设限（2000/1600）。512px 的 JPEG 缩略图
  每张可达上百 KB，2000 条就可能占用数百 MB；同时 `image` 只启用了
  jpeg/png/gif/webp/bmp，TIFF、ICO、QOI 会被判为不支持（415）。
- 缓存上限：新增 `ThumbnailCacheLimits`（条目数 + 总字节数），回收时按 mtime 从旧到新
  删除，直到**两个目标同时满足**；并且始终保留最新的一个条目——单张缩略图超过字节
  目标时若允许清空，缓存会在每个请求上「删光→重新生成」，反而更慢。日志增加
  `remaining_entries` / `remaining_bytes`。
  上限可通过环境变量覆盖：`VFILES_THUMBNAIL_CACHE_MAX_ENTRIES`（默认 2000）、
  `VFILES_THUMBNAIL_CACHE_MAX_MB`（默认 256），清理目标取上限的 80%。
- 格式支持：`image` 增加 `tiff` / `ico` / `qoi`（均为纯 Rust，无需系统库），
  支持列表与 mime 列表同步扩展（`image/tiff`、`image/x-icon`、
  `image/vnd.microsoft.icon`、`image/qoi`）。AVIF 解码需要 dav1d 系统库，
  暂不引入，已记录为后续项。
- 测试：`vfiles-http` 新增 4 个缓存用例（字节超限但条目未超、单张超限时保留最新一条、
  目标值派生）与 2 个格式用例（TIFF/ICO/QOI/PNG 解码为 JPEG + 扩展名/mime 识别；
  SVG/PDF/AVIF 仍被拒绝），`vfiles-config` 新增默认值用例；前端 202 项保持通过。

### 2.48 搜索结果分页（round 41，性能）

- 背景：`/api/files/search` 虽然早就接受 `limit`/`offset`（默认 50、上限 500），但
  响应是裸数组、没有 `has_more`，客户端固定 `limit=500` 一次拉完——大工作区下既浪费
  带宽，又会在超过 500 条时静默截断。
- 后端返回分页信封 `SearchPageDto { items, limit, offset, has_more }`：内部按
  `limit + 1` 取数，多出的一条只用于判定 `has_more`，随后截断——避免为了精确
  `total` 再扫一轮（检索按名称/内容两路查询后在应用层合并排序，统计总数代价高）。
  `limit` 收敛到 `1..=500`。
- 客户端：`searchFiles()` 返回 `{ items, hasMore, limit, offset }`（仍兼容旧版数组
  响应），每页 `SEARCH_PAGE_SIZE = 100`；`useFileSearch` 新增
  `searchHasMore` / `searchLoadingMore` / `loadMoreSearchResults()`，沿用
  `searchSequence` 序号守卫（切词后到达的旧页会被丢弃），`clearSearch` 与新一轮搜索
  都会重置分页状态；`FileBrowser` 的加载哨兵在搜索态下也会继续取下一页。
- 测试：后端新增 `file_search_pages_results_with_has_more`（5 条命中按每页 2 条跑完
  3 页，断言每页不超限、不重复、`has_more` 收敛），并更新 2 处既有断言读信封；
  前端新增 4 个分页用例（追加下一页并续传 offset、无更多时不再请求、切词后丢弃旧页、
  清空搜索重置分页）。
- 冒烟：131 个文件下 `limit=100` 依次请求 offset 0/100 得到 `100/30` 且
  `has_more` 由 true 变 false，`limit=0` 收敛为 1、`limit=9999` 收敛为 500；
  Playwright 实际滚动触发两次请求 `(100,0)` → `(100,100)`，列表由 100 条增长到
  130 条（末行为 `bulk-130.txt`），无控制台报错。

### 2.49 整窗拖放上传（round 42，交互）

- 背景：此前只支持「拖动条目移动到文件夹」（内部拖拽）与上传对话框内的拖放区，
  从桌面把文件拖进浏览器窗口不会有任何反应——主流云盘都支持，是最常见的上传方式。
- 新增 `useWindowFileDrop` 组合式函数：在 window 上监听 dragenter/dragover/
  dragleave/drop，仅当 `dataTransfer.types` 含 `Files`（外部文件）时激活，
  因此内部拖拽移动不受影响；用深度计数避免子元素间的 dragenter/dragleave 让浮层
  闪烁；`dragover` 必须 `preventDefault` 才会收到 drop，`drop` 也阻止默认行为，
  否则浏览器会直接打开文件。弹窗打开时（预览/上传/移动/历史/分享）自动让位。
- 新增 `UploadDropOverlay.vue`：整窗半透明浮层 + 虚线卡片，提示「松开即可上传」
  与目标目录名称；`pointer-events: none` 保证不会挡住落点。
- `FileUploader` 暴露 `addFiles()`，拖入的文件直接进入既有上传队列并打开对话框，
  复用原有的分片/进度/取消逻辑，另给出一条成功提示。
- 测试：新增 6 个组合式函数用例（浮层显隐与深度计数、内部拖拽忽略、文件交接与浮层
  收起、drop 阻止默认、dragover 仅在携带文件时阻止、弹窗期间禁用）、2 个浮层用例、
  2 个 `FileBrowser` 集成用例（拖入后队列出现对应文件、拖入过程显示/移除提示）。
- 冒烟：Playwright 真实拖放（DataTransfer + DragEvent）验证浮层文案为
  「松开即可上传 / 文件会上传到 docs」（当前目录），松手后浮层消失、上传对话框打开
  且队列中出现该文件，无控制台报错。

### 2.50 按钮与进度条的统一（round 43，交互）

- 背景：设计令牌铺开三轮后，仍有 86 处 `is-light`／Bulma 默认按钮保持「浅底方块」
  样式，深色模式下尤其突兀；上传进度用 `is-primary`、下载进度用 `is-info`，
  同一个概念两种颜色。
- 按钮统一（全局，一处生效）：
  - Bulma 的中性按钮（默认 / `is-light`）统一为**幽灵按钮**——透明底、细边框、
    正文文字色，hover 给一层浅底，disabled 统一 45% 透明度；带语义的按钮
    （`is-primary`/`is-link`/`is-danger`/…）保持各自填充样式，因此不需要逐文件改
    86 处调用。
  - Bulma 的 `is-ghost`（行内操作图标）统一为次要文字色（`--vf-text-muted`），
    hover 才变强调色；删除态 hover 使用危险色浅底，不再默认就是蓝/粉。
- 进度条统一：新增 `ProgressBar.vue`（轨道 `--vf-surface-sunken`、填充
  `--vf-accent`、高度 0.5rem、未知总量走不确定态动画、带 `aria-label`），
  上传队列与下载面板都改用它，并删除被取代的 `UploadProgress.vue`；
  下载面板新增 `downloadPercent()` 把字节进度换算成百分比。
- 验证：浅/深色下采样中性按钮均为 `transparent` 底 + `--vf-border` 边框 + 正文色，
  行内图标为 `--vf-text-muted`；`is-primary` 仍是品牌蓝填充 + 白字。冒烟实测
  上传 24MB 文件时出现 `aria-label="上传 big-upload.bin"` 的进度条（高 8px）并成功入库；
  下载同一文件时进度条 `value=33`、轨道 `#f5f8fc`、填充 `#2f6db6`（与上传一致），
  全程无控制台报错。测试新增 3 个 `ProgressBar` 用例。

### 2.51 排序入口统一与移动端视图控件（round 44，交互）

- 背景：排序此前只藏在一级菜单「视图」下拉里；列表表头可点排序，网格视图没有任何
  排序入口；移动端更是**完全没有**视图/排序控件（内容区工具栏在移动端只有搜索）。
- 新增 `SortMenu.vue`：工具栏上的独立排序控件——触发器显示当前字段（`名称` 等）
  并在 `title` 里带上方向，旁边是升/降序切换按钮；菜单里列出四个字段（重复点击当前
  字段等于切换方向，与表头行为一致）、方向选择（移动端用）与「文件夹置顶」。
  排序状态仍存在 `fileView`，与表头点击、视图下拉共用同一份状态。
- `ViewOptions.vue` 只保留显示方式（改为分段控件，激活项用强调色浅底，替换原先的
  `is-link is-light`）、文件夹置顶与缩略图大小；移动端搜索行补上视图与排序两个入口
  （方向按钮在触屏隐藏，方向改由菜单选择，避免搜索行过于拥挤）。
- 文案统一：列表列头改用共享的 `SORT_FIELD_LABELS`，消除同一字段「修改日期」（表头）
  与「修改时间」（排序菜单）两种叫法。
- 测试：新增 5 个 `SortMenu` 用例（字段与方向回显、切换字段、重复点击同字段翻方向、
  方向按钮、文件夹置顶），并把 `FileList` 用例的表头文案同步为「修改时间」。
- 冒烟：桌面依次验证 `排序：名称 升序 → 修改时间 升序 → 修改时间 降序`，关闭文件夹
  置顶后目录参与名称排序；移动端确认排序/视图入口存在且菜单可用（`排序：类型 升序`）；
  网格视图下用同一入口改为按大小排序并生效；全程无控制台报错。

### 2.52 文件详情弹窗（右键 / 长按）（round 45，交互）

- 背景：元数据此前只在桌面端右侧面板可见；移动端**完全没有**任何查看详情的入口，
  桌面端在不显示面板时也无从查看（面板开关关闭或窗口较窄）。
- 抽出 `FileDetailsContent.vue`：图标 / 名称 / 路径 / 类型 / 大小 / 修改与创建时间
  （有提交信息时追加），操作按钮通过 `actions` 插槽由调用方提供。桌面右侧面板与新的
  详情弹窗共用同一份呈现，字段与文案不会再各写一遍。
- 上下文菜单新增「详细信息」：桌面右键与移动端长按（round 29 起已支持）都能打开
  一个详情弹窗，弹窗内提供「预览 / 下载 / 分享」快捷操作；详情弹窗打开时整窗拖放
  自动让位。
- 测试：新增 3 个 `FileDetailsContent` 用例（元数据渲染、目录显示 `--` 且无提交行、
  `actions` 插槽按需渲染）与 1 个 `FileBrowser` 集成用例（右键 → 详细信息 → 弹窗
  含标题、大小与类型）。
- 冒烟：桌面右键菜单出现「详细信息」并打开 `详细信息: main.ts`（类型 代码文件、
  大小 34.0 B、修改/创建时间与三个操作）；移动端长按同一入口，弹窗内容一致；
  两端均无控制台报错。

### 2.53 左侧目录树（round 46，交互）

- 背景：此前只能靠面包屑与「上一级」在目录间移动，深层级来回跳转很费操作；主流
  文件管理器（资源管理器 / Dropbox / 群晖 / 阿里云盘）都有左侧目录树。
- 新增 `DirectoryTree.vue`（宽屏桌面 ≥1280px 显示，中等宽度与移动端隐藏）：
  - 只列目录，按需加载（`getFilesPage` 每层一次、上限 200 条、客户端只保留目录并
    按本地化顺序排序），子目录结果缓存，展开/收起无重复请求；
  - 用「已展开 + 已加载」拍平成带缩进的行，避免递归组件；
  - 当前目录高亮，并在进入深层目录时**自动逐级展开祖先**（每层一次请求，已缓存则跳过）；
  - 点击目录即导航（会先退出搜索态）；拖动条目时目录行成为放置目标，复用既有的
    `handleDropOnFolder` 移动逻辑。
- 布局改为三列：`目录树(232px) | 列表 | 详细信息(280px)`，两列/三列由 `has-tree` /
  `has-details` 控制，中间列始终 `minmax(0, 1fr)`。
- 测试：新增 5 个组件用例（只列目录且顺序正确、展开加载子级并可收起、点击上抛
  `navigate`、当前目录高亮且祖先自动展开、仅拖动时接受放置）与 1 个 `FileBrowser`
  集成用例（宽屏渲染目录树、点击后按路径取数）。
- 冒烟：宽屏下树显示 `全部文件 / 图片 / docs`，展开 docs 得到 `api/guides`，点击 api
  后面包屑变为 `根目录 / docs / api`、树内高亮 api、列表显示 `nested.txt`；
  1100px 与移动端下目录树数量为 0；全程无控制台报错。

### 2.54 缩略图计数指标与默认日志级别（round 47，稳定性）

- 背景：缩略图的解码失败、格式跳过此前只写日志且没有累计值，排障时无法判断量级；
  更严重的是**未设置 `RUST_LOG` 时 `EnvFilter::from_default_env()` 会禁用所有事件**，
  也就是默认配置下服务几乎不输出任何日志（连告警都被吞掉）。
- 计数：`thumbnail.rs` 新增进程内计数（缓存命中 / 生成 / 不支持或过大跳过 / 解码失败 /
  回收条目数与字节数），并在对应分支累加：
  - 跳过与失败分支的日志带上累计值（`skipped_total` / `failed_total`），便于按日志看趋势；
  - 回收成功时累计 `pruned_entries` / `pruned_bytes`。
- 暴露：`GET /api/health` 增加 `thumbnail` 对象（只读快照，无锁），运维可以直接采集；
  健康检查仍保持轻量（仅原子读）。
- 日志默认级别：改为 `RUST_LOG` 未设置时默认 `info`（仍可通过环境变量覆盖），
  启动信息、维护任务结果与缩略图告警默认可见。
- 测试：新增集成用例 `health_reports_thumbnail_counters`（基线计数 → 请求不支持格式的
  缩略图 → 计数 +1，并校验各字段存在）；回收用例补充计数增量断言（并发下用下界）。
- 冒烟：真实服务上依次请求 PNG（`generated:1`）、重复请求（`cache_hits:1`）、
  txt（415，`unsupported:1`）、损坏 PNG（415，`failed:1`）；去掉 `RUST_LOG` 重启后
  默认日志出现 `Server listening ...`、`VFiles server started` 与
  `WARN ... failed to generate thumbnail ... failed_total=1`。

### 2.55 通知改为卡片式提示（round 48，交互）

- 背景：通知此前是 Bulma 的整块 `notification`，**位置 `top: 1rem` 会压住固定顶栏**，
  没有类型图标，批量操作连发多条时会无限堆叠刷屏。
- `Notification.vue` 改为卡片式提示：左侧 3px 语义色竖条 + 类型图标（成功/失败/警告/信息）
  + 文案 + 关闭按钮；位置下移到 `顶栏高度 + 0.75rem`，宽度 `min(24rem, 100vw - 2rem)`；
  容器设为 `role="status"` + `aria-live="polite"`，并支持 `prefers-reduced-motion`。
- `app.store` 增加 `MAX_NOTIFICATIONS = 4`：超出时丢弃最旧的一条，避免批量操作刷屏。
- 测试：新增 3 个通知用例（按通知渲染且带 live region、关闭按钮生效、只保留最近几条）。
- 冒烟：桌面触发重命名冲突得到红色竖条 + 图标的提示（`top=64`，顶栏高 53）；移动端拖入
  文件后的成功提示 `top=64`、宽 358（390-32），不再遮挡顶栏；无控制台报错。

### 2.56 API 错误文案本地化（round 49，交互）

- 背景：服务端返回的领域错误是英文（如重命名冲突提示
  `Path conflict: Path already exists: b.txt`），前端直接展示，与中文界面不一致；
  服务端其实**早就有稳定的 `code` 字段**，只是客户端从未使用。
- 客户端新增 `src/utils/apiErrors.ts`：22 个错误码 → 中文文案映射，`localizeApiError()`
  在识别到错误码时渲染中文，并在服务端提供 `details` 时拼接参数（如冲突路径）；
  未知错误码回退到服务端文案，保证自定义提示不丢失；`extractErrorPayload()`
  同时兼容 `{code,message}` 与 `{data:{...}}` 包裹、以及旧的 `error` 字段。
- 接入三条错误路径：axios 响应拦截器（覆盖绝大多数接口）、`api.service` 的两处原生
  fetch（`putBinary` / `postFormNative`）、`files.service.responseError`
  （预览内容、下载、diff）。调用方原有的中文兜底文案仍然保留。
- 服务端补充结构化信息：`PATH_CONFLICT` 响应新增 `details.path`（从领域错误消息末尾
  提取，提取不到则不编造），客户端据此渲染「目标路径已被占用：b.txt」。
- 测试：客户端新增 6 个用例（已知错误码、带路径参数的冲突、未知错误码回退、空负载兜底、
  两种响应包裹形式、旧 `error` 字段）；服务端新增 2 个用例（路径提取的边界、冲突响应
  的 `details.path`）。
- 冒烟：真实接口 `POST /api/files/move` 冲突返回
  `{"code":"PATH_CONFLICT","details":{"path":"b.txt"}}`；浏览器内重命名冲突的提示为
  **「目标路径已被占用：b.txt」**，无控制台报错。

### 2.57 方向键导航与 Shift 区间选择（round 50，交互）

- 背景：键盘此前只支持 Ctrl/⌘+A、Delete、F2、Enter、Esc，**没有方向键导航**——主流
  文件管理器里上下键移动高亮是最基本的操作。
- `moveActiveRow()`：↑/↓ 逐行移动、Home/End 跳到首尾；只在**真实条目**之间移动
  （`.`/`..` 是导航快捷项，跳过它们，与 Shift 区间选择保持一致）；批量模式下选择跟随
  移动；目标行若尚未渲染（分批渲染）会先补齐渲染范围，再 `scrollIntoView` 滚动到可见。
- Shift + ↑/↓：复用 `useFileSelection` 的区间选择；**第一次按 Shift 时以移动前的活动行
  作为锚点**，因此首按即可选中「原位置 → 新位置」的区间。
- 行与卡片新增 `data-vfiles-path`，供滚动定位；状态栏快捷键提示补充 `↑↓ 移动`。
- 测试：新增 4 个用例（方向键移动与越界不变、Home/End、Shift 区间扩展、Shift 首按锚点）
  以及「输入框内不触发方向键」的回归。
- 冒烟：真实浏览器中 `initial f01 → ↓ f02 → End f08 → Home f01`；Shift+↓ 连续两次
  得到 `[f01,f02]` → `[f01,f02,f03]`；无控制台报错。

### 2.58 内联重命名（round 51，交互）

- 背景：重命名此前一律弹对话框（`promptDialog`），而主流文件管理器都是就地编辑。
- 列表行（桌面/移动）与网格卡片在 `renaming` 为真时把名称替换为输入框：
  - 打开时自动聚焦并**选中主文件名（保留扩展名）**，与资源管理器一致；
  - Enter 提交、Esc 取消、**失焦时若名称有变化则提交**；
  - 输入框上的点击不会触发行点击，避免误开预览/目录。
- `FileBrowser` 用 `renamingPath` 记录正在编辑的条目；F2、右键菜单「重命名」、行内按钮
  都改为打开内联编辑，校验与接口调用沿用原有逻辑（非法名称提示、名称未变化提示、
  成功后刷新并 toast）。
- 修掉一个真实缺陷：取消（Esc）后输入框被移除时浏览器会补发 `blur`，导致**取消仍被提交**；
  两个组件都加了 `renameSettled` 标记，提交/取消后忽略随后的 blur。
- 测试：新增 2 个集成用例（右键菜单进入内联编辑 → 输入 → Enter 调用重命名接口且不弹
  对话框；Esc 取消且不调用接口）。
- 冒烟：列表行内编辑时输入框聚焦、预选主文件名 `report-old`；Enter 后列表更新为
  `report-2026.txt` 并提示「重命名成功」；再进一次用 Esc 取消，名称保持
  `report-2026.txt` 未被改写；网格视图同样通过卡片菜单进入内联编辑并成功改名；
  全程无控制台报错。

### 2.59 网格视图按行方向键导航（round 52，交互）

- 背景：round 50 的方向键导航只在网格视图里按「一项」上下移动，左右键完全无效；
  主流文件管理器的网格/图标视图里上下键是按**一行**移动。
- 新增 `src/utils/gridLayout.ts`：`countRowColumns(tops)` 从卡片纵向偏移量推断首行列数
  （只保留纯计算部分便于单测，允许亚像素误差，空数组返回 1 以免步长退化为 0），
  `columnsFromElements()` 负责从 DOM 取偏移量。
- `moveActiveRow`：网格视图下上下键步长 = 实际渲染的列数；列表与网格都支持左右键
  （左/右 = 前/后一项），Home/End 仍然到首尾；无布局环境（jsdom）自动退化为逐项移动。
- 测试：新增 5 个布局工具用例（首行计数、单行/单项、空数组、亚像素容差）与 1 个
  `FileBrowser` 集成用例（把渲染出的卡片偏移量改成每行 3 张，验证 ↓ 跨一行、→ 跨一项、
  ↑ 回到上一行）；`activeName` 辅助函数同时支持列表行与网格卡片。
- 冒烟：真实浏览器 1280px 宽、每行 4 张卡时，`g01 → ↓ g05 → → g06 → ↑ g02 → End g11`，
  零控制台报错。

### 2.60 搜索分页正确性修复与内容搜索开关（round 53，稳定性/性能）

- **审计发现（真实缺陷）**：搜索分页此前由仓储层的 `ORDER BY created_at DESC LIMIT ? OFFSET ?`
  完成，但文件名命中与内容命中是**两路各自分页**，合并后应用层才按得分排序。
  结果是「下一页」不是上一页的延续：同时开启全文搜索时，文件名命中（得分 1.0）在每一页
  都会把内容命中（0.8）挤出窗口——内容命中从第二页起完全取不到，`has_more` 也随之失真。
- 修复：把分页收敛到**排序之后的唯一位置**——
  - 仓储层不再做 SQL 分页（只保留候选上限：文件名 2000 行、内容候选 500 个文件、
    内容命中 500 条），并加上 `e.path ASC` 作为确定性排序附加键；
  - `SearchService::search` 合并两路结果后按「得分降序 + 路径升序」排序，再统一
    `offset`/`limit` 切片，并限制最多 `MAX_SEARCH_RESULTS = 2000` 条参与分页。
- 回归用例（先证明旧实现失败）：`paged_search_keeps_content_matches_across_pages`
  在修复前只能翻出 2/3 条命中（`second-body.txt` 永远取不到），修复后三页连续返回
  且首条为文件名命中；另加 `search_offset_beyond_results_returns_empty_page` 覆盖越界偏移。
- **可配置化**：`FeatureMatrix.search_content` 此前在 `from_env()` 里硬编码为 `false`，
  部署时无法开启（接口返回 403）。新增 `VFILES_FEATURES_SEARCH_CONTENT`（默认 false），
  开启后 `/api/session/bootstrap` 的 `features.search_content` 为 true，前端「全文搜索」
  才会解锁；服务端对未开启时的越权请求仍返回 403。文档已更新到 `DEPLOYMENT.md`。
- 冒烟：开启开关后按 `limit=1` 翻页得到
  `offset0 needle-name.txt(1.0) → offset1 body-a.txt(0.8, 带命中行) → offset2 body-b.txt(0.8)
  → offset3 空且 has_more=false`，与「单页取全部」结果顺序完全一致；
  `/api/session/bootstrap` 返回 `search_content: true`，前端可正常发起搜索，无控制台报错。

### 2.61 缩略图 AVIF 输出与 Accept 协商（round 54，性能）

- 背景：缩略图此前固定输出 JPEG（256px 约 30KB），而 AVIF 在同画质下体积小得多。
- **格式协商**：`negotiate_thumbnail_format(Accept, allow_avif)` 解析 `Accept`：
  显式声明的格式优先于通配（只写 `image/webp` 的客户端不会被塞 AVIF），
  支持 `q=0` 明确拒绝，缺失或都不匹配时回退 JPEG（兼容性最好）。
  ETag 与磁盘缓存文件名都带上格式后缀（`{blob}-{size}.{ext}`），切换格式不会命中旧内容；
  响应新增 `Vary: accept`，避免中间缓存串味。
- **AVIF 默认关闭**（`VFILES_THUMBNAIL_AVIF`，默认 false）：实测 384px 缩略图
  AVIF 4.8KB vs JPEG 30.3KB（小 6 倍），但纯 Rust 编码器（ravif）首次编码
  约 2.0s（JPEG 0.004s，已用最快档 speed=10），必须由运维按 CPU 余量决定；
  开启后结果落盘缓存，只有首次请求付出代价。
- **WebP 主动不做**：`image` crate 只有无损 WebP 编码器，实测照片类缩略图无损 WebP
  160KB，是 JPEG 的 5.6 倍（渐变图才占优），协商到 WebP 属于性能倒退；
  需要有损 WebP 得引入 libwebp（C 依赖）。已写入 `DEPLOYMENT.md` 的说明。
- 测试：单测覆盖协商决策（浏览器典型 Accept、只写 WebP、显式拒绝 AVIF、通配、
  缺失、`text/html`）与两种格式的编码魔数；集成用例断言默认配置下一律 JPEG 且
  带 `Vary: accept`。顺带修掉 round 47 引入的计数用例并发 flaky（改为增量下界断言）。
- 冒烟（开启 AVIF）：`Accept: image/avif` 返回 `content-type: image/avif` + `vary: accept`，
  缓存目录同时出现 `.avif` 与 `.jpg`；命中缓存后响应 4ms。

### 2.62 快照时间窗口保留 + 两项性能项收尾（round 55，稳定性/性能）

- **§3.1c 审计结论：已完成**（无需改动）。移动流程早已用
  `entry_repo.find_paths(namespace_id, &candidates)` 一次性批量查询全部后代的新路径，
  再在单事务里 `move_entries`；本次核对确认没有遗留的「逐个后代 `find_by_path`」循环，
  仅保留移动源/目标各一次的路径查找。
- **§3.1d 时间窗口保留（新增能力）**：
  - `SnapshotService::prune_snapshots_with_age(keep, older_than)`：只删除
    「**不在最新 `keep` 个之内**」且「**早于 `older_than`**」的快照——两个条件是「与」，
    因此开启时间窗口不会比纯数量策略删得更多；`older_than = None` 时行为与原来完全一致。
  - 维护任务新增 `VFILES_MAINTENANCE_SNAPSHOT_MAX_AGE_DAYS`（默认 0 = 不按时间裁剪）；
    CLI 新增 `prune-snapshots --older-than-days N`。
- **§3.1 预压缩评估（决定不扩大范围）**：
  - `index.html` 1,219B 已经超过 1KB 阈值并被预压缩（`.br` 511B / `.gz` 745B），
    降低阈值只能再收进更小的文件，产物数量翻倍、收益仅数百字节；
  - 前端 dist 里**没有二进制资源**（图标是 SVG，压缩率 28%–34%，已覆盖），而 JPEG/PNG/woff2
    本身已是压缩格式，二次 brotli 通常只有 0–2% 收益，因此**决定不对二进制资源做预压缩**。
- 测试与冒烟：新增单测 `prune_snapshots_respects_the_age_window`（把快照回拨到
  40/50 天前，验证 `keep=10 + 30 天` 不删任何东西、`keep=1 + 30 天` 只删超龄的两条）；
  真实服务上 `--keep 1 --older-than-days 30` 删掉 3 条超龄快照、保留最新一条，
  `--keep 10 --older-than-days 30` 删除 0 条，符合「与」语义。

### 2.63 侧栏存储用量与「最近更新」（round 56，交互/性能）

- 背景：侧栏此前只有目录树；主流云盘还会给出「存储用量」与「最近文件」。
- 后端新增 `GET /api/files/overview`：
  - `EntryRepo::stats()` 用 **SQL 聚合**（文件数 / 目录数 / 总字节数）实现，
    总字节数只累计**当前版本**（`entries` 表没有 `current_version_id` 列，
    用 `ORDER BY version DESC LIMIT 1` 子查询取最新版本），避免把整棵目录树拉进内存；
  - `EntryRepo::recent_files(namespace, limit)` 按最新版本创建时间倒序取前 8 条；
  - 路由 `overview.rs` 需要登录态，返回 `{file_count, directory_count, total_bytes,
    recent_files[{path,name,size_bytes,mime_type,updated_at}]}`。
- 前端新增 `SidebarOverview.vue`：显示存储用量（人类可读）+ 文件/目录计数 +
  最近更新列表（类型图标、相对时间）；加载失败只在本区域提示并提供重试，
  不影响文件列表。点击最近文件会跳到其所在目录并把该行设为活动行
  （实测：点击 `c.bin` 后面包屑变为「根目录 › docs」、`c.bin` 高亮）。
- 侧栏结构：目录树（可滚动）+ 概览（固定高度）包在 `.browser-sidebar` 里，
  文件列表变更（上传/删除/重命名/移动/切换目录）后自动刷新概览。
- 测试：4 个 `SidebarOverview` 用例（渲染用量与最近文件、点击上报路径、
  refreshKey 变化触发重载、失败局部降级）+ 2 个后端集成用例
  （统计与最近文件正确性、未登录 401）。
- 冒烟：真实服务返回 `file_count=3 / directory_count=1 / total_bytes=3018`，
  侧栏显示「2.9 KB · 3 文件 · 1 目录」与按时间倒序的最近文件，无控制台报错。

### 2.64 收藏夹（round 57，交互）

- 新增 `favorites` 表（迁移 `0003_favorites.sql`）：**按 `entry_id` 而非路径**记录，
  外键 `ON DELETE CASCADE`——重命名/移动后收藏依然有效，条目删除后自动清除。
- 后端：`FavoriteRepo`（list / add / remove / contains，均为幂等）+ `SqliteFavoriteRepo`；
  路由 `GET/POST /api/files/favorites`、`DELETE /api/files/favorites?path=...`（删除走查询
  参数，避免依赖 DELETE 请求体）；路径不存在返回 404，未登录返回 401。
- 前端：侧栏 `SidebarOverview` 新增「收藏」区块（星标可一键取消）；右键菜单新增
  「加入收藏 / 取消收藏」（按当前状态切换）；收藏变化后侧栏自动重新拉取。
- 修掉一个开发中发现的**请求死循环**：侧栏加载后会回报收藏集合，若父组件无条件刷新
  就会「加载 → 上报 → 再加载」。现在只有集合真的变化时才刷新。
- 测试：2 个后端集成用例（增删查 + 幂等 + 404 + 401；条目删除后收藏级联清除）
  与 2 个前端用例（收藏列表渲染与取消、点击收藏上报）。
- 冒烟：API 上加入 keep.txt/docs → 重命名 keep.txt 后收藏显示 `renamed.txt`（按 ID 存储
  生效）→ 取消收藏；浏览器里右键「加入收藏」→ 侧栏出现该条目并提示「已加入收藏」→
  菜单变为「取消收藏」→ 侧栏星标取消后列表清空，全程无控制台报错。

### 2.65 上传进度胶囊 + 加载/空状态复核（round 58，交互）

- **上传进度胶囊**：桌面工具栏在「上传」按钮旁显示队列状态——`待上传 N`（有排队）、
  `上传中 x/y`（进行中，带呼吸圆点）、`上传完成 x/y`（全部结束）；点击胶囊重新打开
  上传对话框。这样上传进度不必打开对话框就能看到，与主流云盘一致；
  队列清空后胶囊自动消失。`FileUploader` 新增 `summary`（total/done/failed/active/queued）
  并通过 `defineExpose` 暴露。
- **加载/空状态复核（§3.4 剩余项）**：核对后确认已统一，无需再改——
  - 骨架屏：列表与网格共用 `FileSkeleton`，由 `:variant="viewMode === 'grid' ? 'grid' : 'list'"`
    驱动（桌面/移动两处）；
  - 空状态：桌面与移动共用 `.browser-empty` 样式（空目录 / 无搜索结果两种文案）；
  - 进度条：上传队列与下载面板共用 `ProgressBar`（round 43）；
  - 下载面板仅在存在下载任务时出现，因此不需要空状态。
- 冒烟：真实浏览器中未入队时胶囊不存在；选入一个大文件后显示「待上传 1」，
  上传中显示「上传中 0/1」，关闭对话框后胶囊仍在且点击可重新打开对话框，
  上传完成后胶囊消失且文件出现在列表中，全程无控制台报错。

### 2.66 错误细节结构化（round 59，交互）

- 服务端补齐结构化 `details`：
  - `ApiError::Validation { field, message }` → `{field, reason}`；
  - `DomainError::Validation { message }` 与 `DomainError::Conflict { message }` → `{reason}`；
  - 上传超限改为专用错误码 **`FILE_TOO_LARGE`**（HTTP 413）并带
    `{limit_bytes, size_bytes}`，不再让客户端从英文消息里猜上限。
- 客户端 `apiErrors.ts` 据此渲染：
  - `FILE_TOO_LARGE` → 「文件过大，已超过上限（最大 100 MB）」（本地把字节数格式化）；
  - `VALIDATION_FAILED` + `details.field` → 「输入内容不合法：密码」（内置字段名 → 中文
    标签映射）；**不再把英文 `reason` 拼进用户可见文案**；
  - 未知错误码仍回退服务端文案（兼容自定义提示）。
- 测试：服务端新增结构化 details 用例（handler 内校验返回 `{reason}`、路径冲突返回
  `{path}`）；客户端新增 2 个用例（超限上限格式化、字段名本地化与未知字段回退）。
- 遗留（记入 §3.8）：axum 的 `Json` 提取器在请求体本身非法时返回 422 且**响应体不是**
  我们的错误信封；要统一需要自定义提取器并替换各处理函数的 `Json<T>`，评估后暂不做。

### 2.67 目录分页下沉到 SQL + 统一错误信封（round 60，性能/稳定性）

- **性能（§3.1b 重启调查，找到真正瓶颈）**：先按路线图设想测量「深分页 OFFSET 成本」，
  结果发现 **offset 深浅几乎不影响耗时**，但 `/api/files/list` 在 5 万条目目录下
  **恒定 ~480ms**（`limit=3` 也一样），而 `/api/health` 仅 1.4ms。定位到真正原因：
  分页此前是「仓储取回整目录 + 应用层切片」，每个请求都要物化全部条目与版本信息。
- 修复：
  - `EntryRepo::find_children_page()`：SQL 侧 `LIMIT/OFFSET` + `COUNT(*)`，
    排序 `(kind = 'directory') DESC, path ASC` 与 live tree 的「目录优先 + 名称升序」
    完全一致（同目录下比较完整路径等价于比较名称）；sqlx 要求静态 SQL，因此根目录与
    子目录各写一份语句，均命中 `(namespace_id, path)` 索引；
  - `WorkspaceService::live_children_page()` 复用抽出的 `build_tree_items()`（批量取版本，
    避免 N+1）；`/api/files/list` 与 `/api/files/list/{path}` **统一**走同一个分页实现
    （此前根目录有独立 handler，仍是全量路径）；指定快照（历史版本）时保留整树重建。
- 实测（5 万条目目录，3 次取最快）：`offset=0` **480ms → 40ms**，
  `offset=25000` 53ms、`offset=49800` 70ms；分页连续性、`total`、`has_more`、
  越界空页与「目录优先」均已验证。
- **游标分页（cursor）结论**：分页下沉到 SQL 后，残余的 OFFSET 扫描只占 40→70ms 的
  增长，属于可接受范围；改成 keyset 需要调整 API 契约与客户端，收益有限，
  因此**评估后不做**，改为记录测量数据。
- **统一错误信封（§3.8 遗留）**：新增 `ApiJson<T>` 提取器（内部仍用 `axum::Json`），
  把请求体解析失败转换为 `VALIDATION_FAILED` + `details{field:"body",reason}` + 400，
  12 处 JSON 处理函数全部切换；此前非法 JSON 会得到 axum 自己的 422 且响应体不是
  统一信封，客户端无法本地化。
- 测试：新增「目录分页 SQL 化」集成用例（目录优先、页间连续、total/has_more、
  越界空页、子目录路径）与「非法 JSON 使用统一信封」用例。

### 2.68 应用栏全局搜索（round 61，交互）

- 背景：搜索入口此前只在内容区工具栏，与 Drive/OneDrive 的「顶栏全局搜索」不一致。
- **不改动搜索状态的归属**：搜索状态（结果、分页、过期响应守卫）仍由 `FileBrowser`
  持有，应用栏只通过 store 投递一次「搜索请求」（`appStore.requestSearch` /
  `pendingSearch` / `clearPendingSearch`），`FileBrowser` 消费后填充自己的
  `searchQuery` 并执行既有搜索流程。这样避免了把整套搜索状态提升到页面级的重构风险，
  同时自动复用搜索历史、结果渲染与分页。
- 应用栏：桌面端在品牌与版本胶囊之间加入胶囊式搜索框（带图标、清空按钮与历史
  `datalist`）；提交后写入与文件浏览器相同的 `vfiles.searchHistory` 存储；
  **移动端隐藏**（保留工具栏内已有的搜索行）。
- 测试：新增 store 通道用例（去空格、空查询不入队、消费后清空）与
  `FileBrowser` 消费用例（收到请求后调用搜索接口）。
- 冒烟：桌面端顶栏输入 `needle` 回车后，列表显示 `find-needle.txt`，工具栏搜索框
  同步显示该查询；移动端应用栏搜索槽为 `display: none` 且工具栏搜索行仍在；
  无控制台报错。

### 2.69 FileBrowser 工具栏拆分与规模评估（round 62，稳定性）

- 抽出两个自包含的工具栏组件：
  - `MobileSearchBar.vue`：下拉刷新提示 + 移动端搜索行 + 高级筛选（含原 `.mobile-search-*`
    样式），仅通过 `v-model:*` 与两个事件与父组件交互；
  - `DesktopCommandBar.vue`：导航按钮、视图/排序、详情与批量开关、桌面搜索框、
    上传进度胶囊与上传按钮。
- 结果：`FileBrowser.vue` **2484 → 2301 行**（−183，−7%），新增两个组件共 359 行（含样式）。
- **规模评估（对 < 1500 行目标的结论）**：当前 `FileBrowser.vue` 构成为
  template 739 / script 1275 / style 279 行。即使把剩余模板全部外移，落点仍在 ~1600 行，
  因为**脚本层就是编排层**（refs、composable 装配、约 40 个处理函数）。
  要进一步缩小只能把这套编排搬进 `useBrowserController` 之类的组合式函数，属于高风险的
  大规模重构，收益（行数）与代价（间接层、调试成本）不成比例，因此**决定不做**，
  把目标记为「结构清晰、职责单一、有测试保护」而非单纯的行数指标。
- 测试：265 个前端用例全部保持通过（拆分未改变 DOM 结构与事件契约）。

### 2.70 本轮迭代收尾（round 63，审计）

- §3 待办清零：所有条目要么 `[x]` 完成，要么 `[~]` 经实测/评估后明确不做并写明理由
  （游标分页、AVIF 解码、`FileBrowser.vue` < 1500 行）。
- 最终验证（本轮全量）：
  - Rust：`cargo test --workspace` 16 个测试目标全绿，`cargo clippy --workspace
    --all-targets` 零告警，`cargo fmt --all -- --check` 仅剩历史遗留的
    `crates/vfiles-http/src/frontend.rs`（改动前即存在，未纳入本次范围）；
  - 前端：**42 个文件 / 265 个用例**通过，`vue-tsc`、`eslint` 全通过；`prettier`
    在本次改动过的 25 个文件上全部通过（仓库另有 12 个历史未格式化文件，与本次无关）；
    生产构建 189.7 KB（brotli）；
  - 端到端冒烟（全新数据目录，零控制台报错）：侧栏目录树 / 存储用量 `23.0 B` / 区块标题，
    排序控件回显 `排序：名称 升序`，`End` + `F2` 内联重命名为 `final-report.txt` 并提示
    「重命名成功」，应用栏全局搜索 `item` 命中 `item.txt` 且工具栏搜索框同步，
    右键菜单 9 项（含「加入收藏 / 详细信息」），收藏后侧栏出现 `other.txt` 并提示
    「已加入收藏」，详情弹窗标题 `详细信息: other.txt`，网格视图方向键移动生效。
- 本轮迭代（round 42–63）新增/修复摘要：
  - 性能：缩略图字节上限与格式、搜索分页正确性、目录分页下沉到 SQL
    （5 万条目 480ms → 40ms）、AVIF 输出（默认关闭）；
  - 稳定性：周期性维护、缩略图计数、默认日志级别、时间窗口快照保留、
    统一错误信封（含请求体解析失败）、快照/收藏外键级联；
  - 交互/界面：中性按钮与进度条统一、通知卡片化、排序控件、详情弹窗、目录树、
    存储用量与最近更新、收藏、应用栏全局搜索、内联重命名、方向键导航（列表 + 网格）、
    整窗拖放上传、上传进度胶囊、错误文案本地化；
  - 文档：`DEPLOYMENT.md`（日志级别、缩略图格式、维护与快照保留、功能开关）、
    `API.md`（侧栏聚合与收藏）、`USER_GUIDE.md`（排序、详情、目录树、拖拽上传）。

### 2.71 界面问题修正（round 64，交互）

用户反馈三处界面问题，全部复现并修正：

- **顶部栏多出的搜索框**：移除 round 61 加入的应用栏全局搜索（含 Home 的模板/样式/
  状态、`app.store` 的 `pendingSearch` 通道、`FileBrowser` 的消费 watcher 与对应测试）。
  搜索统一回到内容区工具栏入口，避免同一页面两个搜索框。
- **面包屑下拉「点了没反应」（真因）**：菜单确实渲染了，但面包屑列表为了横向滚动设置了
  `overflow-x: auto`——绝对定位的下拉被这个滚动容器**裁剪**，只剩一条边可见。
  改为把菜单 `Teleport` 到 `body` 并用 `position: fixed` 按触发按钮定位（贴边时自动左收），
  同时补充 resize/scroll 时关闭、点击菜单内部不误判为「外部点击」。
  菜单本身也升级为**按需加载的目录树**（`BreadcrumbTreeMenu.vue`）：展开某层才请求子目录
  并缓存，点击目录即跳转；没有子目录时显示「当前目录没有子文件夹」；
  切换按钮改为**始终显示**，不再因为「当前目录没有子文件夹」而消失。
- **上传按钮另起一行（真因）**：round 62 抽出 `DesktopCommandBar` 后，
  `.desktop-command-group` 的 `display: flex` 仍留在 `FileBrowser.vue` 的 **scoped** 样式里，
  对子组件里的元素不再生效，动作组退化为块级布局。把工具栏相关样式移入
  `DesktopCommandBar.vue`，并固定为单行不换行（搜索框自适应、上传按钮靠右）。
  调整后工具栏高度由 96px 回到 32px，搜索与上传按钮在同一行。
- 顺带修掉：根目录面包屑常驻虚线「放置目标」样式（`dropTarget` 初值为 `""`，恰好等于根
  路径）——初值改为 `null`，只在真正拖动经过时高亮。
- 测试：新增/更新 3 个面包屑用例（下拉渲染在滚动容器之外、点击菜单内部不关闭、
  没有子文件夹时仍保留入口并给出提示）与 2 个 `BreadcrumbTreeMenu` 用例（首层不发请求、
  展开按需加载并上报导航；空目录提示）；移除应用栏搜索的用例。
- 冒烟：顶栏搜索框数量 0；工具栏单行（32px，搜索与上传同一行且上传靠右）；
  面包屑下拉完整可见（命中测试确认三层行均可点击）、展开 `docs` 得到 `api`、
  点击 `api` 后跳转（面包屑 `根目录 docs api`、列表 `n.txt`）；全程无控制台报错。

### 2.72 详情面板可见时隐藏列表「操作」列（round 65，交互）

- 需求：显示右侧「详细信息」面板时，列表里的「操作」列（预览/下载/分享/历史/重命名/
  移动/删除 7 个按钮）与面板中的同名操作完全重复，应隐藏以腾出横向空间。
- 实现：`FileList` / `FileItem` 新增 `showActionColumn`（默认 `true`），
  `FileBrowser` 传入 `:show-action-column="!detailsVisible"`；表头与每行的操作单元格
  同步隐藏，列数保持一致（不会留下空白列）。
- 命名注意：`FileItem` 里原本已有 `showActions` 计算属性（移动端「展开操作」用），
  因此新属性命名为 `showActionColumn` 以免相互覆盖；`FileItem` 用普通 `defineProps`，
  默认值在模板中以 `!== false` 兜底。
- 桌面端实测（1440px）：面板可见时表头为「名称/修改时间/类型/大小」、行内操作按钮 0 个；
  点「隐藏详细信息」后「操作」列与 15 个行内按钮恢复；再次打开又隐藏，全程无控制台报错。
- 测试：新增 2 个 `FileBrowser` 用例（面板可见时无操作列表头且每行 5 个单元格；
  面板隐藏时有表头且每行 6 个单元格）。

### 2.73 修复「老库启动不迁移」导致侧栏报错（round 66，稳定性）

- 现象（用户反馈）：在项目目录 `cargo r -- serve` 启动后，侧栏「存储用量」显示
  「服务器内部错误」，「最近更新」也没有内容。
- 复现与定位：用项目自带的老库启动，`GET /api/files/overview` 返回 200，但
  `GET /api/files/favorites` **返回 500**。查库发现 `_sqlx_migrations` 只有 `[1]`，
  `favorites` 表不存在——因为 **`SqliteMigrations::run` 只在 `vfiles init` 里调用**，
  `serve` 从不执行迁移：用旧版本 init 出来的库，升级二进制后永远不会补上新表/索引。
  侧栏 `SidebarOverview.load()` 用 `Promise.all` 同时请求概览与收藏，收藏一旦失败，
  整段加载失败，于是存储用量与最近更新一起空白。
- 修复：
  - `run_serve` 在连接数据库后执行 `SqliteMigrations::run`（幂等；失败即报错退出，
    不会带着不完整的库对外服务），并补充日志「Running database migrations...
    / Database migrations up to date」；
  - `SidebarOverview` 改为 `Promise.allSettled`：概览与收藏**各自独立降级**，
    收藏接口失败时存储用量与最近更新仍然正常显示（反之亦然）。
- 验证：用项目里那个只有迁移 1 的老库启动，日志显示迁移执行，`_sqlx_migrations`
  变为 `[1, 2, 3]` 且 `favorites` 表存在；`/api/files/favorites` 与
  `/api/files/overview` 均返回 200；浏览器侧栏显示「20.0 MB / 2 文件 · 1 目录」与
  两条真实最近文件，无失败请求、无控制台报错。
- 测试：新增 1 个侧栏用例（收藏请求失败时仍显示用量与最近更新、且不显示错误文案）；
  文档补充升级说明（`serve` 自动迁移）。

### 2.74 修复对话框底部按钮贴在一起（round 67，交互）

- 现象（用户反馈）：移动对话框底部「取消 / 移动到当前目录」两个按钮紧挨在一起。
- 真因：项目只挑选了 Bulma 的 modal 布局样式，**没有包含给底部按钮加间距的规则**
  （构建产物里没有 `.modal-card-foot .button:not(:last-child){margin-inline-end:...}`），
  而 `.modal-card-foot` 自身只有 `display: flex` + 右对齐，于是所有走
  `Modal` 的 `#footer` 插槽的对话框都会出现按钮贴边。
- 修复：在 `Modal.vue` 的 `.modal-card-foot` 上加 `gap: 0.5rem` 与 `flex-wrap: wrap`
  （窄屏自动换行）；因为 `MoveDialog` / `DialogHost` / `ShareDialog` /
  `FileBrowser` 的详情弹窗都复用同一个 footer 元素，一处修复覆盖全部对话框。
- 验证：移动对话框实测 footer `column-gap: 8px`，两按钮同排且间隔 8px
  （取消 right=856、移动到当前目录 left=864）；截图确认视觉正常，无控制台报错。

### 2.75 修复目录树不及时刷新（round 68，交互）

- 现象（用户反馈）：新建目录后，侧栏「全部文件」目录树不刷新，需要收起/展开甚至刷新页面。
- 真因：`DirectoryTree` 对每层子目录做了缓存（`children[path]`），只有首次加载与展开时才
  发请求，没有任何刷新入口——文件列表变了，树完全不知道。
- 修复：`DirectoryTree` 新增 `refreshKey` 属性；`FileBrowser` 传入已有的
  `sidebarVersion`（文件列表每次重新加载、或收藏变化时递增），树在该值变化时
  **重新拉取所有已加载的层级（含根层）**，同时保留用户的展开状态，
  并再次确保当前目录的祖先展开与高亮。加载函数增加 `force` 选项以忽略缓存。
- 验证（真实浏览器，UI 全流程）：
  - 先在树里展开 `existing`（进入缓存）→ 通过右键「在此新建子目录」创建 `ui-child`
    → 目录树**自动**出现 `ui-child`，无需收起/展开或手动刷新，提示「目录创建成功」；
  - 另验证根层：新建 `brand-new` 后树从 `[全部文件, existing]` 变为
    `[全部文件, brand-new, existing]`；
  - 全程无控制台报错。
- 测试：新增 2 个 `DirectoryTree` 用例（refreshKey 变化后根层与已展开层都刷新且保留展开
  状态；刷新后当前目录仍保持展开与高亮）。

### 2.76 移除列表「. / ..」并优化新建文件夹入口（round 69，交互）

- **移除 `.` / `..` 两行**：列表此前在真实条目前注入两个合成条目（`uiRole: self/parent`），
  与主流云盘不一致（返回上一层已有面包屑、目录树与「上一级」按钮三条路径）。
  改动：`navigationListItems` 只返回真实条目并删除合成条目构造；
  `FileItem`/`FileCard` 去掉 `isNavigationShortcut` / `isSelfShortcut` / `isParentShortcut`
  分支、`uiRole` 判断与相关样式；排序工具、网格过滤、选择逻辑、图标组件里的
  `uiRole` 判断一并清理（`sortBrowserItems` 保持为常规排序）。
- **新建文件夹入口**：
  - 桌面工具栏在「上传」左侧新增**文字按钮「新建文件夹」**（图标 + 文案，单行不换行，
    搜索框自适应收缩；实测 1440/1280/1100px 工具栏均为 32px 单行）；
  - 移动端底栏「导航」组新增**新建文件夹按钮**，通过 `appStore.requestCreateDirectory()`
    自增计数单向投递（底栏在 Home，创建对话框在 FileBrowser），FileBrowser 监听后
    在当前位置打开同一个「新建目录」对话框；
  - 保留原有入口：目录行的右键「在此新建子目录」、空目录里的「新建文件夹」按钮，
    以及移动端长按菜单。
- 验证（真实浏览器）：
  - 桌面列表只显示 `docs`、`readme.md`（无 `.`/`..`）；点击工具栏「新建文件夹」→
    对话框标题「新建目录」→ 创建 `新建的目录` 后列表与**目录树同时出现**该目录，
    提示「目录创建成功」；
  - 移动端底栏按钮存在、合成条目计数为 0，创建 `手机目录` 后列表出现；
  - 两端口均无控制台报错。
- 测试：新增 2 个 `FileBrowser` 用例（列表不再渲染 `.`/`..`；工具栏按钮走既有
  「新建目录」提示流程），并更新排序用例（不再有快捷项分组）。

### 2.77 修复移动端顶栏折叠菜单（round 70，交互）

- 现象（用户反馈）：移动端顶栏右侧折叠菜单有问题——展开后只有两个没有文字说明的图标
  （显示器图标 / 头像），点开下拉还会被截断。
- 真因（三处叠加）：
  1. `ThemeToggle` 的主题名称与账号名都带 Bulma 的 `is-hidden-touch`，在移动端被隐藏，
     菜单里只剩图标；
  2. 展开的 `.navbar-menu` 在移动端被 Bulma 设了 `overflow: auto`，行内下拉
     （外观选项）被这个滚动容器裁剪，只能看到一行；
  3. 下拉没有提升层级，会被后续行遮住。
- 修复（全部在 `Home.vue` 的移动端样式与账号标记里）：
  - 账号名去掉 `is-hidden-touch`（折叠状态整个菜单本就不可见）；主题名称用
    `:deep(.theme-toggle-label)` 覆写 `display: inline !important`——ThemeToggle 是子组件，
    scoped 样式必须用 `:deep` 才能命中其内部元素（第一次改漏了这点，标签仍隐藏）；
  - `.navbar-menu.is-active { overflow: visible }`，让展开的下拉完整显示；
  - 展开的 `.dropdown.is-active` 提升 `z-index`，不再被下一行盖住；
  - 菜单行改为整行可点、左对齐、带分隔线，并补上**点击菜单外部 / Esc 关闭**
    （点菜单内部不误关）。
- 验证（390×844）：菜单行显示「跟随系统」「账号」；点开后外观面板完整显示
  「跟随系统 / 浅色 / 深色」，选「深色」后 `data-theme=dark` 生效；账号下拉显示
  「账号 / 本地模式」；点汉堡、点外部、按 Esc 均能关闭，点菜单内部不关闭；
  桌面端（1440）主题名、账号名与版本胶囊保持可见；全程无控制台报错。

### 2.78 修复网格卡片「…」菜单被裁剪（round 71，交互）

- 现象（用户反馈）：网格视图下点卡片「…」弹出的菜单显示不正确（只露出前几项）。
- 真因：`.file-card-menu` 当时嵌在 `.file-card-thumb` **内部**，而缩略图容器为裁圆角设了
  `overflow: hidden`——菜单面板高约 241px、从卡片顶部往下延伸，被这个容器裁到只剩
  3 项（预览/历史版本/重命名），其余被下方内容遮住（命中测试落在状态栏上）。
- 修复：把整个 `.file-card-menu` 移到 `.file-card-thumb` **之外**、作为 `.file-card`
  的直接子元素。`.file-card` 本身是 `position: relative; overflow: visible`，
  绝对定位的菜单位置不变（仍贴卡片右上角），但不再被裁剪。
- 验证（1440，网格视图）：
  - 文件卡片菜单 7 项：预览/历史版本/重命名/移动/下载/分享/删除；点「预览」打开
    「预览: readme.md」；
  - 目录卡片菜单：在此新建子目录/打开/重命名/移动/下载/分享/删除；点「打开」进入 docs
    （面包屑变为「根目录 docs」）；
  - 裁剪链只剩页面滚动容器（正常），面板底部命中测试为「删除」按钮而不再是状态栏；
  - 点菜单外部关闭、点卡片缩略图关闭且不误触发预览；无控制台报错。
- 测试：新增 `FileCard` 用例 3 个（菜单必须在缩略图容器之外、文件卡片 7 项、
  目录卡片为目录专属条目）。

### 2.79 修复「显示方式」按钮状态显示错误（round 72，交互）

- 现象（用户反馈）：切换显示方式的「列表 / 网格」两个按钮状态不正确。
- 真因：`ViewOptions.vue` 里有一条早期规则
  `.view-options.dropdown.is-active .vf-ghost-button { … }`，本意是**下拉展开时高亮
  「视图」触发器**，但它是**后代选择器**，而面板也在 `.view-options` 内部——下拉一打开，
  面板里的「列表 / 网格」两个按钮同时被点亮（实测两者背景都是
  `rgba(47,109,182,0.08)`、文字都是 `rgb(24,77,155)`），真正的激活项无法分辨。
- 修复：改为直接子选择器 `> .dropdown-trigger .vf-ghost-button`，只高亮触发器本身，
  并加注释说明为什么不能用后代选择器。
- 验证（真实浏览器，逐状态采样计算样式）：
  - 列表模式：列表 = 激活（强调色浅底 + 强调色文字），网格 = 未激活（透明底 + 次要文字色）；
  - 切到网格后（等过渡结束）：网格激活、列表未激活，`is-active` 与 `aria-pressed` 同步；
  - 再切回列表恢复正确；下拉里的其它控件（文件夹置顶、缩略图大小）不受影响；
  - 无控制台报错。截图确认「网格」高亮、「列表」为普通态。
- 说明：这类样式问题无法用 jsdom 单测覆盖（不解析 SCSS），因此以浏览器计算样式采样
  作为证据；顺带检查了其它组件，没有同类「后代选择器误伤」的规则。

### 2.80 批量操作条与工具栏的间距（round 73，交互）

- 现象（用户反馈）：批量选择的工具栏与上方工具栏之间没有空白，看着像半成品。
- 实测：批量条紧贴命令栏（`gap = 0`），并且是一条**通栏**的强调色底 + 底部边框，
  视觉上像是工具栏多出一行没画完。
- 修复（`BatchActionBar.vue`）：改为独立**圆角面板**——`margin: 0.5rem 0 0.25rem`、
  `padding: 0.5rem 0.75rem`、`border: 1px solid var(--vf-accent-soft-strong)`、
  `border-radius: var(--vf-radius)`，去掉原来的通栏底边框。
- 验证（1440px）：命令栏底 176 → 批量条顶 184（**间距 8px**），批量条底 233 → 列表顶 247
  （间距 14px）；截图确认已是独立圆角面板，与上下两个区域都有留白，无控制台报错。
- 说明：移动端批量操作在底栏面板里（另一处布局），不受影响。

### 2.81 修复目录列表返回整棵子树（round 74，稳定性）

- 现象（用户反馈）：目录树里「全部文件」激活状态不对；**不同层次的同名文件夹选中后会同时高亮**。
- 定位：树里出现**重复行**（`a/docs/x` 出现两次），同名目录（`a/docs` 与 `b/docs`）的子孙
  行复用同一路径，于是「同一个 path 在多处渲染」——高亮自然一起亮。
  根因在服务端：round 60 把目录分页下沉到 SQL 时用了**范围匹配**
  `e.path >= 'a/' AND e.path < 'a0'`，它匹配的是**整棵子树**而不是直接子条目，
  于是 `/api/files/list/a` 返回了 `a/docs`、`a/docs/x`、`a/docs/deep.txt`。
- 修复：改用与原有 `find_children` 一致的**直接子条目**判定——
  `e.path LIKE 'parent/%' AND instr(substr(e.path, length('parent/') + 1), '/') = 0`，
  COUNT 与分页查询同步修改（`total` 也只统计直接子条目）。
- 回归测试：新增 `directory_listing_returns_only_direct_children`（先证明旧实现失败：
  返回 `["a/docs","a/docs/deep.txt","a/docs/x","a/top.txt"]`，修复后为 `["a/docs","a/top.txt"]`，
  并校验 `total=2` 与更深一层的 `a/docs` 只含 `a/docs/deep.txt`、`a/docs/x`）。
- 验证（真实浏览器，含同名嵌套目录 `a/docs` 与 `b/docs`）：
  - 列表接口修复后 `/a` → `total 1`（仅 `docs`）、`/a/docs` → `total 1`（仅 `x`）；
  - 目录树 **8 行无重复**（修复前 `a/docs/x`、`b/docs/x` 各出现两次）；
  - 点击 `b/docs` 只有 `b/docs` 高亮，再点 `a/docs` 只有 `a/docs` 高亮，两者不再同时点亮；
  - 全程无控制台报错。

### 2.82 历史版本对话框重设计（round 75，交互）

- 目标：用现代主流设计重做历史版本对话框的呈现与交互。
- 重设计要点：
  - **两栏布局**：宽屏（≥900px）左侧版本列表、右侧详情面板（预览 / 对比 / 空态提示），
    列表与详情各自滚动并限制高度；窄屏单栏，详情面板自动排到列表上方并提供「收起详情」。
    为此给 `Modal` 增加 `wide` 属性（`width: min(960px, 92vw)`），仅版本历史使用。
  - **紧凑版本行**：时间线轨道 + 圆点（当前版本为强调色实心并带光晕）；
    行内为「当前版本 / 第 N 版」胶囊 + 变更类型标签 + 更新消息 + 元信息
    （**相对时间**（悬停显示绝对时间）· 作者 · 短哈希）。
  - **带文字的操作按钮**：预览 / 对比 / 下载 / 恢复（ghost 按钮，不再是无标签的彩色图标），
    当前版本显示「这是当前版本」且不提供恢复。
  - **去掉样板文案**：自定义备注不再附加「这条更新消息会显示在版本历史里」等重复说明，
    由 `describeCommitMessage` 直接省略（创建/删除/重命名同理）。
  - 顶部摘要「N 个版本 · 当前 <短哈希>」；「加载更多」显示剩余数量。
- 修掉一个真实缺陷：`viewVersion()` 之前只关闭预览、不关闭对比，导致「对比」之后点
  「预览」仍显示对比结果；现在两个面板互斥。
- 测试：新增 5 个 `VersionHistory` 组件用例（版本数与当前版本摘要、当前版本标记且无恢复、
  操作按钮带文字标签、预览/对比面板互斥切换、恢复前二次确认）；更新
  `commit-message` 相关断言。
- 验证（真实浏览器，桌面 1440 + 移动 390，均无控制台报错）：
  - 桌面：摘要 `6 个版本 · 当前 1196ff62`；当前版本行高亮、无恢复；其余行为
    `第 5 版`… 且含恢复；点「对比」右侧显示 diff（110 字符），再点「预览」切换为版本内容
    （`line 1`）；列表仅高亮 1 行；
  - 移动：详情面板置顶显示，四个操作按钮同行；空态提示与「收起详情」可用。

### 2.83 FTP(S) 批量导入（round 76，新功能）

- 目标：为「一次导入几百到几万个文件」提供 FTP 通道，客户端可递归上传整个目录。
- 研究阶段发现的关键阻碍（先修前置能力，再落协议）：
  1. **每次写入都会写一份全命名空间快照**（`collect_snapshot_state` 读取全部条目与版本），
     逐文件提交在批量导入时退化为 O(文件数 × 条目数)；
  2. 流式写入没有大小上限（HTTP 层自己计数，FTP 若不复用会绕过 `max_file_size_bytes`）；
  3. 客户端断开会把 `tmp/blob-upload-*.tmp` 留在磁盘上；
  4. 凭据校验、命名空间解析与登录限流都锁在 HTTP 层。
- 交付内容：
  - **应用层**：`ImportBatch`（新增）按文件数阈值/会话结束提交**一次**全量快照，支持
    `batch` / `per-file` / `off` 三种策略；`import_file_stream` 用 `take(limit+1)` 做流式限额
    并在超限时回滚 blob；`store_blob_stream` 在读写失败时清理临时文件；
    `AuthService::verify_credentials`、`NamespaceService`、`LoginAttemptLimiter` 下沉供多协议复用。
  - **新 crate `vfiles-ftp`**：基于 `unftp-core` 的 `StorageBackend` 实现（list/get/put/mkd/rmd/del/
    rename/cwd）、复用 Web 凭据与角色白名单的认证器、路径沙箱（`..` 在根处夹取，`CDUP` 仍可用）、
    领域错误→FTP 应答映射、`IngestStats` 计数；自建 accept 循环（`Server::service`）以共享停机信号。
  - **配置与装配**：`VFILES_FTP_*`（默认关闭）含主机/端口/被动端口段与通告地址/角色白名单/并发与
    空闲超时/FTPS 证书/快照策略，`validate()` 拒绝端口冲突、只配证书不配私钥、要求 FTPS 无证书、
    未知快照策略、以及「未开启认证却开启 FTP」；`FeatureMatrix.ftp_enabled`、`GET /api/files/ftp-info`
    与 `/api/health` 的 `ftp` 计数块；`serve` 内 FTP 与 HTTP 共享 watch 停机信号。
  - **前端**：上传对话框内「用 FTP 批量导入」折叠卡片（服务器/端口/账号/加密状态/被动端口段/
    可复制 curl 示例），仅在功能开关开启时渲染。
- 验证证据：
  - Rust：19 个测试目标全绿（含 5 个真实 TCP + suppaftp 的端到端用例：上传/下载/改列/重命名/删除、
    错误口令与路径穿越、超限回滚、非空目录 RMD 失败），clippy 全目标零告警；
  - 前端：45 文件 / 287 用例（含 5 个 FTP 卡片用例），`vue-tsc`/`eslint`/`prettier`/构建通过；
  - 真实二进制冒烟：`VFILES_FTP_ENABLED=true` 后 `curl -T` 与 `curl --ftp-create-dirs` 上传成功，
    网页端可见文件与目录、历史记录出现「FTP 上传: hello.txt」，`ftp-info` 与 `health.ftp` 数据正确，
    SIGTERM 停止监听且无残留端口；默认配置下不监听任何 FTP 端口；
  - **规模对比**（1200 个 256B 文件，同一台机器，Python `ftplib` 递归上传）：

    | 快照策略 | 耗时 | 吞吐 | 快照数 |
    | --- | --- | --- | --- |
    | `per-file` | 72.9s | 16.5 文件/秒 | 1201 |
    | `batch`（默认，阈值 200） | 53.8s | 22.3 文件/秒 | 7 |

    即默认策略下**快照数降低约 170 倍、导入耗时下降约 26%**；文件少时两种策略差异不明显
    （300 文件时约 20 文件/秒），说明收益来自消除快照的平方级写入。
- 后续可选项：`vfiles ftp` 子命令（FTP 与 HTTP 分离部署）与 `vfiles import <目录>`（服务端本地
  目录导入，复用同一批次 API，无网络暴露）。

### 2.84 FTP 默认开启 + `vfiles import` 服务端目录导入（round 77，新功能）

- **FTP 改为默认开启**：`VFILES_FTP_ENABLED` 未设置时默认 `true`（认证开启场景），
  默认部署即在 `2121` 提供批量导入，网页端上传对话框的连接信息卡片随之默认可见。
  为避免「默认开启」带来的副作用，同时做了三件事：
  1. **认证关闭时自动降级**：未显式设置 → FTP 自动停用并打 WARN（不阻塞 `serve`）；
     显式设置 `true` 而认证关闭 → 直接报配置错误并说明原因。
     开关解析抽成纯函数 `ConfigLoader::resolve_ftp_enabled`，测试不再依赖环境变量（避免并发测试互相干扰）。
  2. **端口冲突不再致命**：FTP 监听失败只记录 ERROR 并继续提供 HTTP 服务（默认开启后
     端口被占用不应拖垮站点）。
  3. `vf`/文档同步：`.env.example`、README、DEPLOYMENT、API、USER_GUIDE 均改为「默认开启，
     不需要时显式关闭」，并保留明文告警。
- **新增 `vfiles import <SOURCE>`**（服务端本地目录导入，不开放任何网络端口）：
  - 复用 `ImportBatch`，因此同样按阈值合并快照；新增 `ImportBatch::create_directory`
    （空目录/嵌套目录在批次内创建，不单独提交快照）与 `skip_unchanged` 选项
    （内容与当前版本一致时跳过，不生成新版本）；
  - 目录结构原样保留（含空目录），符号链接一律跳过，`--target/--owner/--snapshot-mode/
    --flush-files/--force/--dry-run/--max-file-size-bytes/--exclude-hidden` 可调；
  - 有失败项时以非零退出码结束并打印前 20 条错误，便于脚本判断。
- 验证：
  - 真实二进制冒烟：默认配置下 `2121` 自动监听且 `curl -T` 上传成功；认证关闭时不显式设置
    FTP 会打印「FTP 批量导入自动停用」且 `2121` 无监听、HTTP 正常；认证关闭 + 显式开启 FTP
    报出可读的配置错误；第二个实例遇到端口占用时只报 ERROR，HTTP `health` 仍为 200；
  - `vfiles import` 冒烟（多级目录 + 空目录 + 隐藏文件 + 同名不同层 + 符号链接）：
    预览 `9 个文件 / 6 个目录 / 102 字节 / 跳过 1 个符号链接`；实际导入 `--target docs`
    写入 9 个文件与 6 个目录、`--flush-files 3` 恰好 3 次快照、9 个文件只产生 8 个 blob
    （同内容去重）；**二次导入 0 写入 / 9 跳过 / 0 快照**；改动一个文件后再导入为
    1 写入 / 8 跳过 / 1 快照；
  - Rust：新增 4 个 `import_cmd` 用例（遍历与排序/跳过符号链接、导入+幂等+`--force`、
    `--dry-run` 不写入、源目录缺失与用户不存在报错）与 3 个配置用例（默认开启、开关解析矩阵、
    校验规则），前端无需改动。

### 2.85 优化 FTP 连接信息中的服务器地址（round 78，交互）

- 现象：上传对话框里的 FTP 地址不可靠——默认配置下 `VFILES_HTTP_PUBLIC_BASE_URL` 是
  `http://localhost:3000`，而原实现按「配置优先」把它当作展示地址，于是远程客户端拿到
  `localhost`；`VFILES_FTP_HOST` 为 `0.0.0.0` 时甚至可能展示通配地址。
- 修复：`/api/files/ftp-info` 改为**按「客户端最可能连得上」挑选地址**，并返回
  `host_source`（来源）与 `remote_reachable`（是否可能被其它机器访问）：
  1. `VFILES_FTP_PASSIVE_HOST`（管理员显式指定）；
  2. 当前请求的 `Host` 头（客户端正是用它访问 Web）；
  3. `VFILES_HTTP_PUBLIC_BASE_URL` 主机名；
  4. `VFILES_FTP_HOST`（具体网卡地址时）；
  5. 通过默认路由探测到的本机地址（`UdpSocket::connect` 选路，不发包，结果缓存）；
  6. 回环地址（`remote_reachable=false`，页面提示「仅本机可访问」）。
  关键点是**远程可达地址永远优先于回环地址**：写测试时先按「配置优先」实现，
  两个用例立刻失败（`localhost` 压过了内网地址与绑定地址），据此修正为两级筛选。
- 前端：卡片新增地址来源标签（管理员指定 / 当前访问地址 / 站点地址 / 服务绑定地址 /
  本机网卡地址 / 本机回环），`remote_reachable=false` 时显示醒目提示并建议配置
  `VFILES_FTP_PASSIVE_HOST`；IPv6 字面量在示例命令中带方括号。
- 验证（真实服务 + 浏览器，本机内网地址 192.168.5.201）：
  - 默认配置下用 `Host: localhost` / `127.0.0.1` 访问 → 返回 `192.168.5.201`
    （`detected_address`，`remote_reachable=true`），页面显示「服务器 本机网卡地址
    192.168.5.201」且**无**告警；
  - `Host: 192.168.5.201` → 展示该地址（`request`）；`Host: files.example.com` → 展示域名；
  - `VFILES_FTP_PASSIVE_HOST=ftp.example.com` → 两种 Host 都展示 `ftp.example.com`（优先级最高）；
  - 单元测试覆盖：通配地址不展示、`localhost`/`127.0.0.1`/`[::1]` 判定为仅本机、IPv6 方括号、
    以及「显式回环地址被可用地址取代」。

### 2.86 文件列表交互优化（round 79，交互）

- 现象（用户反馈）：文件多时，若关掉「操作」列（开着详情面板），下载/重命名/移动等操作不方便——
  只剩右键菜单或「先点选行、再移到右侧面板点按钮」两条路径。
- 参考主流网盘（Drive / OneDrive）补齐三种入口：
  1. **行内「⋯」菜单**：`FileItem` 在名称单元格右侧新增按钮，hover / 聚焦 / 选中 / 触屏时出现，
     点击复用既有 `ContextMenu`（同一套菜单项），弹出位置贴着该行下方；
  2. **行内复选框**：非批量模式下也提供（hover 出现），勾选即自动进入选择态并显示操作条，
     不再要求先点工具栏的「批量选择」；
  3. **操作条吸顶**：选择条从工具栏内部移到**列表列内部**并 `position: sticky`，
     长列表滚动到很下面时依然可达（放在工具栏内会被工具栏的高度限制，放在 layout 外层则会横跨详情列）。
- 顺带修复：`.home` 的 `overflow-x: hidden` 会让它成为滚动容器，导致内部 `sticky` 失效，
  改为 `overflow-x: clip`（并按 `@supports` 回退）；
- 键盘可达性：新增 `Shift+F10` / 菜单键打开当前行菜单（Windows 惯例）。
- 测试：新增 `FileItem` 3 个用例（关闭操作列时仍有「⋯」并可发出菜单事件、非批量模式提供复选框、
  批量模式复选框常驻），`FileBrowser` 3 个用例（行内菜单在操作列隐藏时可用且菜单项含下载/重命名/移动、
  勾选即出现操作条、`Shift+F10` 打开菜单、操作条位于列表列内）。
- 验证（真实浏览器，61 个文件的会话）：操作列隐藏（表头计数 0）时，「⋯」默认 `opacity: 0`、
  hover 后为 `1`，菜单 9 项且出现在该行下方，点「重命名」直接进入内联重命名；
  勾选一行后操作条自动出现（`已选 1 项 | 全选当前视图 | 清空选择 | 下载 | 移动 | 重命名 | 删除`）；
  滚到 `file-55` 后操作条仍固定在 `y=58`（视口内、按钮可点），且宽度限定在列表列（x=313、宽 766），
  不覆盖详情面板；`Shift+F10` 与方向键后 `Shift+F10` 均能打开菜单；全程无控制台报错。

### 2.87 界面迭代（第 1 轮）：详细信息面板重塑（round 80，视觉/交互）

- 目标（本轮迭代主题）：让界面更贴近同类系统（Google Drive / OneDrive / Dropbox）的主流视觉与交互。
- 本轮范围：右侧「详细信息」面板的信息架构与视觉，以及顺带的吸顶行为。
- 改动：
  1. **预览区**：图片显示真实缩略图（`thumbnailUrl(path, {size:320, commit})`，加载失败回退类型图标），
     其它类型显示较矮的类型图标块（避免大面积留白）；
  2. **标题区**：名称 + 路径 + 「×」关闭按钮（收起面板后列表自动恢复「操作」列）；
  3. **分区化**：原来的「两列键值卡片 + 两列操作网格」改为「详细信息」「操作」两个分区，
     键值改为左标签 / 右取值的行式布局（更适合窄面板扫读），并新增**位置**（所在目录）字段；
  4. **主次分明的操作**：主操作（预览 / 打开）用新增的通用按钮修饰符 `.vf-ghost-button.is-primary`
     （主色实底）通栏展示，下载/分享/历史版本/重命名/移动 两列排布，删除单独红色通栏；
  5. **吸顶 + 内部滚动**：面板 `position: sticky`（`align-self: start` + `max-height`），
     长列表滚动时信息与操作始终可见——与上一轮做的「批量操作条吸顶」保持一致。
- 复用约束：面板与移动端/右键菜单的详情弹窗共用 `FileDetailsContent`，通过 `previewUrl` /
  `showClose` 两个可选属性区分；同时保留 `desktop-details-name` 等既有类名作为稳定选择器。
- 验证（真实浏览器，含真实 PNG 样本）：文本文件显示类型图标、图片显示缩略图（`imagePreview: 1`）、
  分区为「详细信息 / 操作」、键值行含「位置」、关闭按钮存在且点击后面板消失并恢复「操作」列、
  滚动 700px 后面板仍在视口（`y=115`）；移动端弹窗渲染同一内容且不含关闭按钮（标题栏已有）；
  全程无控制台报错。
- 测试：`FileDetailsContent` 新增 3 例（缩略图与失败回退、关闭按钮与事件、位置字段），
  `FileDetailsPanel` 新增 2 例（仅图片请求缩略图且带 size=320、关闭事件）。

### 2.88 界面迭代（第 2 轮）：侧栏存储用量占比条（round 81，视觉）

- 目标：主流网盘的侧栏都会给出「存储空间」的分类构成，VFiles 之前只有一个数字。
- 改动：
  1. **领域层**：新增 `FileCategory`（document / image / video / audio / other）与 `CategoryUsage`，
     `EntryRepo` 增加 `stats_by_category`；
  2. **仓储层**：SQL 单次聚合，按当前版本的 `entry_versions.content_type` 前缀归类
     （文档含 text/*、PDF、JSON/XML、Office 与 ODF 系列），返回「字节数 + 文件数」并按字节倒序。
     实现时先写成了 `ev.mime_type`，SQLite 报列不存在 → 路由按设计降级为空数组，
     测试立刻暴露（`document category should exist`），改为正确的 `content_type`；
  3. **HTTP**：`GET /api/files/overview` 增加 `categories` 字段（分类失败不影响总数与最近文件）；
  4. **前端**：侧栏「存储用量」下方新增**分段占比条 + 图例**（颜色、标签、各项大小、悬停显示文件数），
     极小分类保留 2% 最小宽度，空工作区显示灰轨道；新增 5 组 `--vf-chart-*` 颜色令牌（浅色/深色各一套）。
- 验证：
  - 接口（真实服务，混合内容）：`total_bytes: 54168`，
    `categories: [video 40000/1, other 9000/1, document 4209/2, image 959/2]`；
  - 浏览器（浅色 + 深色，1440px）：占比条宽 208px，四段宽度 73.84% / 16.62% / 7.77% / 2%（图片被夹到最小值），
    颜色分别取 `--vf-chart-video/other/document/image`（深色主题自动换用高对比度配色）；
    图例为「视频 39.1 KB / 其它 8.8 KB / 文档 4.1 KB / 图片 959.0 B」，
    无障碍标签为「存储构成：视频 39.1 KB，其它 8.8 KB，文档 4.1 KB，图片 959.0 B」；两个主题均无控制台报错。
- 测试：Rust 新增 2 个接口用例（分类字节数与倒序、空命名空间返回空数组），
  前端新增 4 个侧栏用例（占比与图例、最小宽度、空轨道、旧服务端缺字段兼容）。

### 2.89 界面迭代（第 3 轮）：桌面应用外壳与工具栏收敛（round 82，视觉/交互）

- 目标：主流网盘（Drive / OneDrive）的文件区是**固定外壳 + 列表内部滚动**，工具栏、列头、
  状态栏常驻；VFiles 之前整页滚动，滚到列表深处时工具栏、列头与状态栏都会滚出视野。
- 改动：
  1. **应用外壳**（`Home.vue` 桌面媒体查询）：`.home` / `.home-content` 固定 `100dvh` 且不滚动，
     `.home-main-container` → `.home-browser-shell` → `.file-browser` → `.file-browser-box`
     逐级 `flex:1 + min-height:0`，页脚固定在底部；
  2. **列表内部滚动**（`FileBrowser.vue`）：`.desktop-content-layout` 增加
     `grid-template-rows: minmax(0, 1fr)`（否则 grid 行按内容撑开），`.desktop-list-shell`
     成为唯一滚动容器并 `sticky` 列头；
  3. **列头图标**：「名称 ▲/▼」改为 `IconChevronUp/Down` 小箭头（主色），与主流一致；
  4. **菜单去重**：视图菜单删掉与排序菜单重复的「文件夹置顶」（主流把它归为排序选项），
     视图菜单只保留显示方式与缩略图大小。
- 排障记录（值得留存的两处坑）：
  - 第一版外壳不生效：同一媒体查询里后出现的 `.home-browser-shell { display: block }`
    覆盖了新加的 `display: flex`，flex 链断裂、盒子按内容长到 2201px；
  - 第二版外壳生效但列表仍不可滚：Bulma 的 `.table-container` 默认 `overflow-y: hidden`
    把长表格裁掉，桌面端改为 `overflow: visible`，由列表壳统一滚动。
- 验证（真实服务，81 个文件，1440×900）：
  - `pageScrollable: false`、`listScrollable: true`，多次滚到底后 `listScrollTop: 3351`、
    首屏可见行由「设计稿」变为「文件-070.txt」、末行可见（`lastVisible: true`）；
  - 列头 / 工具栏 / 状态栏位置完全不变（`headerStayed/toolbarStayed/statusStayed: true`）；
  - 列头指示器为 `svg`（不再是 ▲/▼ 文本）；视图菜单不含「文件夹置顶」、排序菜单仍含该项
    （`名称 | 排序方式 | 名称 | 修改时间 | 大小 | 类型 | 升序 | 降序 | 文件夹置顶`）；
  - 移动端（390×844）保持整页滚动：`pageScrollable: true`、底栏可见、`scrollY: 600` 生效；
  - 桌面与移动端均无控制台报错。
- 测试：新增 2 例（列头排序指示器为 svg 且不再出现 ▲/▼；视图菜单不含「文件夹置顶」而排序菜单包含）。

### 2.90 界面迭代（第 4 轮）：空状态与错误态统一（round 83，视觉/交互）

- 目标：主流网盘在「空目录 / 无结果 / 加载失败」时都用同一套「插画 + 标题 + 说明 + 主操作」，
  VFiles 之前空目录是裸图标、搜索空结果是一行灰字、加载失败是 Bulma 的 `notification is-danger`。
- 改动：
  1. 新增通用组件 `components/common/EmptyState.vue`：圆形插画底座（中性色 / `tone="error"` 危险色）、
     标题、说明、操作插槽，支持 `compact` 紧凑模式；
  2. 四个状态全部改用它：桌面空目录（上传文件 / 新建文件夹）、桌面搜索无结果、
     桌面加载失败（错误文案 + 重试）、移动端空目录与搜索无结果、详情面板「未选择任何条目」；
  3. **内容搜索无结果时新增「改为文件名搜索」一键切换**（保留关键字，仅切换搜索类型），
     并在说明里点明「内容搜索只匹配文件内容」——这是主流网盘常见的引导；
  4. 空目录的主操作改用主色按钮（`.vf-ghost-button.is-primary`），与详情面板主操作保持一致。
- **顺带修掉的真实缺陷**：验证错误态时发现提示渲染成 `[object Object]`。根因是
  `ApiService.extractErrorMessage` 对 `{ error: { message } }` 这种网关常见形态会返回**对象**，
  被 `new ApiError(object)` 转换成 `"[object Object]"`。修复：`extractErrorPayload` 支持
  `{error:{code,message,details}}` 嵌套形态；`extractErrorMessage` 同样读取嵌套与 `data` 包裹形态，
  且只接受字符串（非字符串一律忽略），兜底固定为字符串。
- 验证（真实服务 + 路由拦截，1440×900）：
  - 空目录：`.empty-state` 存在、插画底座 `border-radius: 50%`、标题「此文件夹为空」、
    操作 `["上传文件", "新建文件夹"]`；
  - 内容搜索无结果：说明为「换个关键字，或改用文件名搜索（内容搜索只匹配文件内容）」、
    操作 `["改为文件名搜索", "清空搜索"]`；点击后说明变为「换个关键字，或检查是否限制了搜索范围」
    且替代按钮消失；
  - 加载失败：`.empty-state.is-error`、插画底色 `rgb(254,236,240)`、图标 `rgb(204,15,53)`、
    标题「加载失败」、提示为服务端文案「服务器内部错误」（修复后不再是 `[object Object]`）、含「重试」；
  - 全程无控制台报错。
- 测试：新增 `EmptyState` 3 例；`FileBrowser` 新增 3 例（空目录结构与操作、错误态与重试、
  内容搜索引导与切换）；`apiErrors` 新增 2 例（嵌套错误体解析、非字符串 message 忽略）。

### 2.91 界面迭代（第 5 轮）：移动对话框重做（round 84，视觉/交互）

- 目标：与主流网盘（Drive/OneDrive）的「移动到」选择器对齐。
- 改前问题（截图审计）：一列「待移动 N 项」大盒子 + chips、根目录/上一级两个 Bulma 小按钮、
  「当前目标目录」盒子、提示行、常驻的「当前目录可作为目标 / 暂不可作为目标」状态卡、
  目录行还重复显示 `/path`；信息重复、层级混乱、垂直空间浪费。
- 改后结构：
  1. **待移动摘要**压缩成一行（主条目图标 + 名称 / 多选时「A、B、C… 共 N 项」）；
  2. **目标选择器**：路径面板（「↑ 上一级」+ 面包屑「根目录 / 文档」，当前层加粗）+
     可滚动的子目录列表（文件夹图标 + 名称 + 右侧 chevron，进入下一层）；
  3. **状态下行**：只有当当前目录不可用时才出现一行警示（如「“报告.txt”已经在当前目录」），
     替代原来的常驻状态卡；确认按钮带目标名「移动到「文档」」，不可用时禁用并在 title 说明原因；
  4. 空目录与错误态复用第 4 轮的 `EmptyState`（「此处没有子文件夹」/「目录加载失败 + 重试」）；
  5. 底部按钮统一为 `vf-ghost-button`（取消 / 主色确认），与详情面板、批量操作条一致。
- 顺带改进：`FileTypeIcon` 与 `fileIconKind` 的入参放宽为 `FileIconSource`（名称 + 类型 + 可选 MIME），
  使只持有部分字段的场景（移动对话框条目摘要）也能复用同一套图标映射。
- 验证（真实服务 1440×900 + 移动端 390×844）：
  - 根目录：面包屑 `["根目录"]`、列表 `["图片","文档"]`、确认按钮禁用并给出原因（条目已在根目录）；
  - 进入「文档」：面包屑 `["根目录","文档"]`、当前层「文档」加粗、确认按钮为 **「移动到「文档」」且可用**；
  - 返回根目录：警示行「“报告.txt”已经在当前目录」出现、确认按钮回到禁用；
  - 移动端：面包屑与列表正常、底部按钮为「取消 / 移动到当前目录」，无控制台报错。
- 测试：新增 5 个 `MoveDialog` 用例（摘要与仅列目录、进入子目录并可确认、当前目录不可用时的提示与禁用、
  待移动目录置灰、空/错误态），前端 48 文件 / 320 用例通过。

### 2.92 界面迭代（第 6 轮）：分享对话框重做（round 85，视觉/交互）

- 目标：把分享流程做成主流网盘的两步式（配置 → 结果），并统一按钮语言。
- 改前问题（截图审计）：用一个**只读输入框**展示「文件路径」（内容是文件名，语义奇怪）；
  底部是 Bulma `button is-primary`（与详情面板/批量条/移动对话框的 ghost/primary 体系不一致）；
  生成链接后只能复制，缺少「打开」「停止分享」，且结果区与表单区在同一堆 field 里。
- 改后结构：
  1. **条目行**：文件类型图标 + 名称 + 路径（新增可选 `file` 属性，由 FileBrowser 传入选中条目；
     仍保留 `filePath` 以兼容旧调用）；
  2. **表单态**：只保留「链接有效期」下拉（1 小时/1 天/7 天/30 天，默认 7 天）+ 一行说明；
  3. **结果态**：只读链接输入框 + 主色「复制」按钮（点击后就地变「已复制」并给出绿色提示条）、
     有效期 chip、**打开链接**（新标签页）、**停止分享**（调用新增的
     `filesService.disableShare` → `DELETE /api/shares/{code}`，成功后回到表单态）；
  4. 关闭即重置（避免下次打开残留上次链接）；错误改为行内提示（`role="alert"`）；
  5. 标题改为「分享「文件名」」，底部按钮统一 `vf-ghost-button`（关闭 / 主色生成链接）。
- 验证（真实服务 1440×900）：表单态标题「分享「报告.txt」」、条目行「报告.txt / /文档/报告.txt」、
  **无只读路径输入框**、有效期 4 个选项、底部「关闭 / 生成链接」；生成后链接
  `http://localhost:18690/s/abc9010a`、`readonly=true`、chip「有效期至 2026/09/28 23:52」、
  操作「打开链接 / 停止分享」；点击复制后按钮变「已复制」且出现提示条；
  点「停止分享」后 `hasLink=false` 且回到「生成链接」；全程无控制台报错。
- 测试：新增 5 个 `ShareDialog` 用例（条目行展示且无路径输入框、生成链接与复制/打开/停止、
  停止分享回到表单、创建失败的行内报错、关闭后重置）。

### 2.93 界面迭代（第 7 轮）：工具栏上传分裂按钮（round 86，交互）

- 目标：对齐主流网盘的上传入口（主按钮 + 下拉选择上传方式），同时把「上传文件夹」提升到工具栏。
- 改动：
  1. `DesktopCommandBar` 的「上传」改为**分裂按钮**：主按钮（上传文件）+ 箭头下拉，
     菜单为「上传文件 / 上传文件夹 / 新建文件夹」（第三项与工具条上独立的「新建文件夹」按钮等价，
     满足上一轮对新建入口可见性的要求）；菜单支持点击外部与 Esc 收起；
  2. 新增 `upload-files` / `upload-folder` 两个事件，`FileBrowser` 用
     `openUploader('files' | 'directory')` 统一处理，并把选择意图透传给上传对话框；
  3. `DropZone` 新增 `initialPick`：打开对话框时直接弹出文件或目录选择器
     （目录选择器为 `webkitdirectory`，可整目录导入并保留相对路径）；
  4. 关闭对话框时清空选择意图，避免下次打开误弹。
- **浏览器验证抓到的一个真实缺陷**：Modal 的内容是**常驻挂载**的（只切换显示），
  所以最初用 `onMounted` 触发选择器只会在首屏生效——菜单里点「上传文件夹」不会弹目录选择器。
  改为 `watch(initialPick)`（`immediate: true`）后按属性变化触发，并补了一个
  「属性从 null 变为 directory」的用例锁定该行为。
- 验证（真实服务 1440×900，用 Playwright 的 `filechooser` 事件断言真实选择器）：
  - 主按钮文案「上传」、箭头按钮存在、独立的「新建文件夹」按钮仍在；
  - 展开菜单：`["上传文件", "上传文件夹", "新建文件夹"]`；
  - 点「上传文件夹」→ 上传对话框打开且 `filechooser` 事件的元素带 `webkitdirectory`（`multiple=true`）；
  - 点主按钮 → 打开文件选择器（无 `webkitdirectory`）；
  - Esc 可关闭菜单；全程无控制台报错。
- 测试：新增 `DesktopCommandBar` 3 例（分裂按钮与菜单项、三个事件、Esc/点击外部收起）、
  `DropZone` 4 例（默认不弹、属性变化时弹目录/文件选择器），前端 51 文件 / 331 用例通过。

### 2.94 界面迭代（第 8 轮）：搜索结果筛选与所在目录（round 87，交互）

- 目标：主流网盘搜索完成后会给出「结果数量 + 类型筛选」，并在每条结果上标明所在位置；
  VFiles 之前只有一行「搜索结果：N 项（文件名）」。
- 改动：
  1. 新增 `components/file-browser/SearchResultToolbar.vue`：左侧结果摘要
     （「找到 N 项」「找到 N 项 · 已显示 M 项」「找到 N 项 · 文档 M 项」），
     右侧类型 chips（全部 / 文件夹 / 文档 / 图片 / 视频 / 音频 / 其它）带数量，
     只展示非空类型，全部同类型时自动隐藏 chip 行；
  2. 分类口径复用文件图标的 `fileIconKind`（folder / image / video / audio /
     text+code+pdf → 文档 / 其余 → 其它），保证筛选与列表图标一致；
  3. `useFileSearch` 新增 `resultFilter` + `setResultFilter`，新一轮搜索与清空搜索都会重置；
  4. 筛选结果参与既有的排序与分页计算（`filteredSearchResults` → `sortedSearchResults`），
     因此列表、网格、状态栏计数与「加载更多」自动一致；
  5. `FileList` / `FileItem`（桌面列表）与 `FileGrid` / `FileCard`（网格）新增 `showLocation`，
     搜索结果在名称下方显示所在目录（根目录显示 `/`）；移动端搜索结果也使用同一工具条。
- 验证（真实服务 1440×900，含子目录同名关键字命中）：
  - 未筛选：摘要「找到 3 项」，chips `全部 3 / 文档 2 / 图片 1`（全部激活），
    行显示「星云会议.txt → /」「星云计划.md → /项目」「星云图.png → /图片」；
  - 点「文档」：摘要「找到 3 项 · 文档 2 项」，只剩两行且所在目录为 `/`、`/项目`；
  - 点「图片」：摘要「找到 3 项 · 图片 1 项」，只剩 `/图片` 那一行；
  - 切到网格视图：3 张卡片、所在目录 `/`、`/项目`、`/图片` 均显示；
  - 控制台仅有测试数据导致的缩略图 415（假 PNG），无其它报错。
- 测试：新增 `SearchResultToolbar` 5 例（摘要与计数、点击上报、筛选后的摘要与空类型隐藏、
  单一类型隐藏 chip 行、已显示数量提示）；另更新 2 处旧断言，前端 52 文件 / 337 用例通过。

### 2.95 界面迭代（第 9 轮）：预览弹窗改为灯箱（round 88，视觉/交互）

- 目标：把预览做成主流网盘/图床的灯箱：顶部操作条、左右悬浮切换、图片缩放旋转、快捷键提示。
- 改前问题（截图审计）：底部一排 Bulma 小按钮（复制/上一张/下一张）；
  没有下载、定位、全屏；图片无法缩放旋转；不支持的类型用 Bulma 警告框。
- 改后结构：
  1. **顶部工具条**：文件图标 + 名称 + 大小；右侧「所在文件夹 / 下载 / 全屏」，
     代码与文本额外提供「复制内容」（就地反馈「已复制」）；模态标题简化为「预览」（不再重复文件名）；
  2. **灯箱切换**：预览区左右两侧悬浮圆形箭头（禁用态半透明），底部「当前 / 总数」；
     键盘 ←/→ 行为沿用 `useFilePreview`；
  3. **图片缩放旋转**（组件内部状态，不影响父级）：滚轮缩放、缩放/适应/旋转控件，
     键盘 +/-、0、R，放大后可拖动平移，换图时自动复位；缩放范围 25%–400%，
     底部实时显示百分比；
  4. **全屏**：对预览容器调用 Fullscreen API，并监听 `fullscreenchange` 同步图标语义；
  5. **空/错误状态**：改用第 4 轮的 `EmptyState`（「预览失败」+ 重试；「暂不支持在线预览」+ 下载）；
  6. 父组件接上 `@download`（进入既有下载队列）与 `@reveal`
     （关闭预览 → 跳到所在目录 → 高亮该行）。
- 验证（真实服务 1440×900，真实 PNG/Markdown/TS 文件）：
  - 图片：顶栏「照片.png 935.0 B」+ 操作 `["所在文件夹","下载","全屏"]`、悬浮箭头 2 个、
    位置「3 / 3」、快捷键提示「←→ 切换 +- 缩放 · 0 适应 · R 旋转 Esc 关闭」、缩放条 4 个按钮；
    两次放大 → 150%（`scale(1.5)`），左旋 → `rotate(270deg)`，键盘 0 → 复位 100%/`rotate(0deg)`；
  - 代码文件：位置「1 / 3」且出现「复制内容」，键盘 → 切到「说明.md 2 / 3」；
  - 下载：点击后下载队列面板出现「下载队列 1 项 … 说明.md 文件 完成」；
  - 所在文件夹：预览关闭且目标行带上选中高亮（`is-row-selected: ["说明.md"]`）；
  - 全程无控制台报错。
- 测试：`FilePreviewModal` 用例从 7 个扩到 12 个（新增下载/定位事件、按钮+键盘缩放旋转、
  滚轮缩放、缩放条仅图片显示；并更新不支持类型、复制按钮与悬浮箭头三处旧断言）。

### 2.96 界面迭代（第 10 轮）：可拖拽列宽与表头悬停（round 89，交互）

- 目标：补齐主流网盘桌面表格的最后一块标准交互——可拖拽列宽（并记住偏好）与表头悬停反馈。
- 改动：
  1. **列宽偏好**进入 `fileView` store：`columnWidths`（name/modified/type/size）、
     `setColumnWidth`（夹在 88–640px）、`resetColumnWidth`，并与视图模式/排序一起持久化到
     localStorage（含旧数据的兼容读取）；
  2. `FileList` 用 `<colgroup>` + 每列 `width` 驱动布局，表头右侧新增拖拽手柄
     （`role="separator"`、可聚焦、`aria-label="调整「名称」列宽"`）：
     鼠标/触摸拖拽、双击恢复默认、键盘 ←/→ 微调 16px、Home 恢复；
  3. **保持 FileList 无状态**：列宽通过 `column-widths` prop 传入、通过
     `resize-column` 上抛（与既有的 `sort-field`/`sort-direction` 一致），
     FileBrowser 负责写 store —— 这样组件测试不需要 pinia，职责也更清晰；
  4. 表头悬停高亮（th 背景 + 排序文字加深），手柄悬停变主色 2px 竖线。
- **浏览器验证发现的问题**：手柄最初放在两列交界处（`right: -4px`），
  命中区域被相邻 th 抢走，导致双击/点击无响应；改到本列内部（`right: 0`）后正常。
- 验证（真实服务 1440×900）：
  - 4 个手柄；拖动名称列 −80px：320 → 240；双击恢复 320；
  - 聚焦「修改时间」手柄按两次 →：150 → 182（2×16）；
  - **刷新后仍为** 320 / 182 / 130 / 96（localStorage 持久化生效）；
  - 悬停手柄：竖线 1px→2px、颜色变主色 `rgb(47,109,182)`；悬停表头：th 背景变
    `rgba(47,109,182,0.06)`；光标为 `col-resize`；全程无控制台报错。
- 测试：`FileList` 新增 2 例（手柄数量/标签/拖拽与双击与键盘上报、按 prop 渲染列宽），
  `fileView` store 新增 2 例（夹取范围与重置、持久化与恢复），前端 52 文件 / 345 用例通过。

### 2.97 界面迭代（第 11 轮）：上传队列面板重做（round 90，视觉/交互）

- 目标：对齐主流网盘的上传面板：概览 + 整体进度 + 紧凑行 + 逐条重试/取消 + 批量操作。
- 改前问题（截图审计）：每个文件是一个 Bulma `box`，内含 level 布局、彩色 tag 状态、
  常驻的「备注 / 更新消息」标签输入与小进度条；没有整体进度、没有重试、批量操作只在模态底部
  （取消全部），失败后只能整批重来。
- 改后结构：
  1. `FileUploader` 新增**队列概览头部**：标题「待上传 N 项」+ 状态计数
     （排队 / 上传中 / 完成 / 失败 / 取消，失败用危险色）+ **整体进度条**
     （仅统计已开始上传的条目）+ 批量操作「重试失败项 / 清除已完成 / 全部取消」（按需出现）；
  2. `UploadQueue` 每行改为**紧凑行**：文件类型图标、文件名、相对目录标签、大小与状态
     （「上传中 42%」/「已完成」/「上传失败」/「已取消」/「排队中」），右侧操作
     「取消」（排队/上传中）、「重试」+ 移除图标（失败/取消）；
  3. 仅在 `queued` 时显示版本备注输入（占位文案改为「版本备注（可选…）」），
     上传中显示进度条，完成行不显示进度条、文件名转灰；
  4. 新增 `retry` 事件与 `retryItem`/`retryFailed`（重新入队，若无其它上传中则立即继续），
     以及 `clearCompleted`；按钮统一为 `vf-ghost-button`/`vf-icon-button`。
- 验证（真实服务 1440×900，Playwright 拦截 PUT 分片制造失败）：
  - 排队态：标题「待上传 1 项」、副标题「排队 1」、行状态「排队中」、操作「取消」、头部「全部取消」；
  - 失败态：行状态「上传失败」+ 原因「网络错误，请检查连接」+ 操作「重试 / 从列表移除」，
    副标题「失败 1」、头部「重试失败项」（见截图）；
  - 点「重试」并放行请求后：队列清空、文件出现在列表中；
  - 正常上传：队列清空、对话框关闭、列表出现新文件；全程无控制台报错。
- 测试：新增 `UploadQueue` 5 例（紧凑行要素、目录标签、进度仅在传、重试/移除事件、失败原因与备注编辑），
  前端 53 文件 / 350 用例通过。

### 2.98 界面迭代（第 12 轮）：用户管理页与主界面视觉对齐（round 91，视觉/交互）

- 目标：管理页此前是独立的 Bulma 风格（section/container/level、条纹表格、彩色 tag、
  整列输入框、danger 通知），与文件浏览器完全不像同一个产品。
- 改动：
  1. **卡片式外壳**：居中卡片 + 页头（标题「用户管理」、「共 N 位用户」/「当前显示 M 位」）、
     搜索框（用户名/邮箱/角色中文名，带清空按钮）与刷新按钮（旋转反馈）；
  2. **角色筛选 chips**：全部 / 管理员 / 管理者 / 普通用户 + 人数，只显示实际存在的角色，
     筛选后为空时给出「没有找到匹配的用户」+「清除筛选」；
  3. **表格升级**：头像 + 用户名（自己的账号标注「当前账号」）、状态改为彩色圆点 + 文案、
     创建时间改用共享的 `formatRelativeDate`（悬停显示原始时间戳，原先直接展示 ISO 字符串）；
  4. **邮箱按需编辑**：默认只读文本 + 悬停出现的铅笔按钮，点击进入行内编辑（保存 / 取消，
     Enter 保存、Esc 取消），不再让整列都是输入框；
  5. **状态与按钮统一**：加载骨架行、`EmptyState`（加载失败 + 重试 / 暂无用户 / 筛选无结果）、
     `vf-ghost-button` / `vf-icon-button`，能力提示改为行内黄色说明条。
- 验证（真实服务 1280×900，4 个不同角色用户，浅色 + 深色）：
  - 页头「用户管理 / 共 4 位用户」，chips `全部 4 / 管理员 1 / 管理者 1 / 普通用户 2`；
  - 关键字「alice」→ 1 行且副标题「共 4 位用户 · 当前显示 1 位」；点「普通用户」→ carol、alice 两行；
  - 铅笔 → 行内输入框值 `alice@example.com`、按钮「保存 / 取消」，Esc 取消后输入框消失；
  - 创建时间显示为「今天 00:17」（title 为 ISO 原值）；两个主题均无控制台报错。
- 测试：`AdminUsers` 用例重写为 3 个（行内编辑邮箱 + 强制下线、关键字/角色筛选、相对时间格式），
  前端 53 文件 / 352 用例通过。

### 2.99 界面迭代（第 13 轮）：认证页统一外壳（round 92，视觉）

- 目标：登录页是 Bulma 的 box + tabs + notification，找回/重置页是另一个没有品牌的裸 box，
  三个流程视觉不一致，且与主界面的控件语言脱节。
- 改动：
  1. 新增 `components/auth/AuthShell.vue`：全屏居中布局、品牌（logo + 名称 + 副标题）、
     卡片（可选标题/说明）、页脚备案、右上角主题切换，登录/找回/重置共用；
  2. 新增 `components/auth/AuthNotice.vue`：信息 / 警告 / 错误 / 成功四种语气的行内提示条，
     取代 Bulma 的 `notification`/`help`；
  3. 登录页：模式切换由 Bulma tabs 改为**分段控件**（活动项白底 + 主色文字），
     输入框加前置图标与统一 `auth-*` 样式，主按钮改为 `vf-ghost-button is-primary`，
     「忘记密码？」改为文字链接，注册关闭提示改为提示条；
  4. 找回/重置页：套用外壳与同一套表单控件（重置 token 支持从 `?token=` 自动带入）。
- **浏览器验证发现的缺陷**：输入框图标不可见——Bulma 的 `.input` 本身是 `position: relative`，
  与绝对定位的图标同处一个层叠上下文且 DOM 顺序在后，图标被输入框底色盖住；
  给图标加 `z-index: 1` 后正常（管理页搜索框一并加固）。注意 `elementFromPoint` 对这种
  `pointer-events: none` 的装饰元素不可靠，最终以截图为准。
- 验证（真实服务 1280×900 浅色/深色 + 390×844 移动端）：
  - 登录：品牌与副标题、分段控件 `登录 / 邮箱验证码 / 注册`（登录激活）、输入图标 2 个、
    主题切换存在；卡片底色浅色白 / 深色 `rgb(26,29,35)`；
  - 邮箱验证码模式：占位 `user@example.com`、`6 位验证码`，且有「发送验证码」按钮；
  - 注册模式：占位含邮箱与密码、按钮「注册」、注册提示文案；
  - 找回页：标题/说明/「发送重置邮件」；重置页：标题与三个字段，`?token=abc` 自动带入；
  - 移动端：卡片宽 366 / 视口 390，不溢出、无横向滚动；三种页面均无控制台报错。
- 测试：`Login` 新增 1 例（分段控件切换模式时字段随之切换），前端 53 文件 / 353 用例通过。

### 3.0 界面迭代（第 14 轮）：新增「我的分享」管理页（round 93，功能 + 视觉）

- 目标：主流网盘都有「我的分享 / 链接管理」入口（可查看有效期与访问次数、停止分享），
  VFiles 之前只能对单个文件生成/停止链接，没有集中管理界面。
- 后端（分享列表补充条目信息）：
  1. 领域层新增 `ShareWithEntry { share, entry_name, entry_path, entry_kind }` 与仓储方法
     `find_shares_with_entry_by_user`；
  2. SQLite 用 `JOIN entries` 一次查出条目路径/类型（**注意：`entries` 表没有 name 列**，
     名称由路径末段推导——首版按 `e.name` 查询直接 500，被新增的接口用例当场抓出）；
  3. `ShareDto` 增加 `entry_name` / `entry_path` / `entry_kind`，列表接口改用新方法。
- 前端：
  1. `filesService.listShares()` 与 `ShareLink` 类型；
  2. 新增 `/shares` 页面「我的分享」：卡片式布局、条目图标/名称/路径、
     有效期徽章（长期有效 / 有效期至 … / 已过期）、访问次数与最近访问、
     链接输入框（聚焦即全选）、复制 / 打开 / 停止（二次确认后从列表移除）、
     加载骨架、空状态与失败重试（复用 `EmptyState`）；
  3. 顶栏账号菜单新增「我的分享」入口（分享功能开启且已登录时显示）。
- 验证（真实服务 1280×900）：
  - `GET /api/share/shares` 返回 `entry_name` / `entry_path` / `entry_kind`
    （`文档` → directory、`合同.md` → file）；
  - 页面副标题「共 4 个链接」，每行显示徽章「有效期至 2026/09/29」、`访问 0 次`、
    链接 `http://127.0.0.1:18770/s/c2a413be`、操作「复制 / 打开 / 停止」；
  - 点复制 → 按钮变「已复制」；点停止并确认 → 该行移除、副标题变为「共 3 个链接」；
  - 无控制台报错。
- 测试：Rust 新增接口用例 `share_list_includes_entry_metadata`（文件 + 目录两种分享的
  名称/路径/类型断言）；前端新增 `SharedLinks` 5 例（列表要素、复制/打开、停止后移除、
  过期标记、空/错误态），前端 54 文件 / 358 用例、19 个 Rust 目标与 clippy 全绿。

### 3.1 分享有效期新增「1 年 / 永久」（round 96，功能，用户请求）

- 需求：分享对话框的有效期
  （原为 1 小时 / 1 天 / 7 天 / 30 天）增加「一年」与「永久」两个选项。
- 实现：
  1. `ShareDialog` 的有效期改为数据驱动（`ttlOptions`），新增 `31536000`（1 年）与 `0`（永久）；
  2. 永久选项**不发送 `expires_at`**（服务端收到缺省字段即视为无过期），
     提示语切换为「永久链接不会自动失效，需要手动停止分享。」，结果区显示「永久有效」
     而不是「有效期至 …」；
  3. 服务端无需改动：`create_share` 早已接受缺省/`null` 的 `expires_at`；
     「我的分享」页面对 `expires_at` 为空的链接已显示「长期有效」。
- 验证（真实服务 1280×900）：下拉为 `1 小时 / 1 天 / 7 天 / 30 天 / 1 年 / 永久`；
  永久 → chip「永久有效」、提示语切换、请求体为 `{"path":"合同.md"}`（无 `expires_at`），
  列表接口返回 `expires_at: null`，「我的分享」徽章显示「长期有效」；
  1 年 → chip「有效期至 2027/09/22 00:39」，请求体 `expires_at: 2027-09-21T16:39:14Z`；
  全程无控制台报错。
- 测试：`ShareDialog` 新增 2 例（选项列表与永久语义、1 年到期时间上报），
  前端 55 文件 / 364 用例通过。

### 3.2 界面迭代（第 16 轮）：版本历史状态统一 + 保留说明（round 97，视觉）

- 目标：版本历史是少数仍在使用 Bulma 通知的界面（`notification is-danger/is-warning`），
  空态也是自己画的一个灰图标；主流网盘会在这里说明「历史保留多久、能做什么」。
- 改动：
  1. 列表加载失败 → `EmptyState`（error 语气）+「重试」（点击重新拉取历史）；
  2. 无历史 → `EmptyState`「暂无历史记录」+「文件每次上传新版本后，都会在这里留下记录」；
  3. diff 加载失败 → 行内黄色提示条（`.history-inline-note`）；预览失败 → `EmptyState`（error）；
     不支持预览 → `EmptyState`「暂不支持在线预览」+「下载此版本」按钮；
  4. 详情面板空态 → `EmptyState`「查看某个版本 / 在左侧版本上选择「预览」或「对比」…」；
  5. 列表上方新增**保留说明**一行：「历史版本会长期保留，可随时预览、对比、下载或恢复；
     删除该文件会同时删除这些历史。」
- 验证（真实服务 1280×900，含 2 个版本的真实文件）：
  - 列表显示「1 个版本 · 当前 5e8dd47d」与保留说明文案；
  - 详情面板空态为「查看某个版本」；点击「对比」后 diff 正常渲染（`.diff-text`）；
  - 组件内 `.notification` 数量为 **0**（页面上唯一的 notification 是全局「登录成功」toast，
    位于对话框之外）；无控制台报错。
- 测试：`VersionHistory` 新增 2 例（保留说明文案、空态与失败态使用共享组件且提供重试），
  前端 55 文件 / 366 用例通过。

### 3.3 界面迭代（第 17 轮）：移动端细节打磨（round 98，视觉/交互）

- 目标：移动端（390×844）此前是「能用」而非「好用」：底部栏只有图标（触屏没有 hover），
  提示条固定在顶部会盖住面包屑与搜索行，行内大小用胶囊标签与桌面不一致。
- 改动：
  1. **底部栏改为图标 + 文字**（更多 / 上一级 / 首页 / 新建 / 上传 / 刷新），按钮高度 44px
     满足触达标准；
  2. **提示条移动端移到底部**（底部栏上方，考虑安全区与浏览器工具栏偏移 `--vv-bottom`），
     桌面仍固定右上；
  3. **行内元信息**去掉 `tag` 胶囊，改为弱化文本「1.0 B · 今天 00:43」，
     最新提交信息也改为弱化文本（截断），与桌面列表统一。
- **浏览器验证发现的真实布局缺陷**：底部栏 5 个按钮此前是固定宽度（2.4rem + 我的
  `min-width: 3.2rem`），总宽 255px 超过面板 253px → 换行后与「更多」重叠
  （实测「更多」x=40、「上一级」x=46，几乎完全重合，截图里看起来只有 5 个按钮）。
  改为面板内按钮 `flex: 1 1 0; min-width: 0; flex-wrap: nowrap` 后，实测按钮 x 依次为
  40 / 79 / 134 / 188 / 243 / 298（等分且互不重叠），行高 56px。
- 验证（真实服务，iPhone 13 视口）：
  - 底部栏 6 项均带文字、44px 高、无重叠、无横向溢出；
  - 提示条底边 586 ≤ 底部栏顶边 608（不再遮挡内容）；
  - 行内元信息为「1.0 B · 今天 00:43」；无控制台报错。
- 测试：`FileItem` 新增 2 例（元信息为纯文本且不再使用胶囊、提交信息为弱化文本），
  前端 55 文件 / 368 用例通过。

### 3.4 界面迭代（第 18 轮）：多选时的详情面板摘要（round 99，交互）

- 目标：主流网盘在右侧信息面板里对多选给出「已选 N 项 + 总大小 + 批量操作」，
  VFiles 之前无论选多少条都只显示单条详情。
- 改动：
  1. `FileDetailsPanel` 新增 `selection` 属性：多于 1 项时渲染选择摘要
     （「已选择 N 项」+「共 X · N 个目录」+ 前 5 个条目名 + 「还有 N 项」），
     少于等于 1 项时保持原有单条详情；
  2. 摘要里的操作复用父组件的批量处理器（`download-selection` / `move-selection` /
     `delete-selection` / `select-all` / `clear-selection`），避免重复实现批量逻辑；
  3. `FileBrowser` 新增 `selectedItems`（按列表顺序取选中项）传入面板。
- **浏览器验证发现的真实缺陷**：非批量模式下悬停出现的行复选框写死了 `:checked="false"`，
  点击后状态确实被记录（行高亮、批量条出现），但复选框看起来**永远没勾上**，
  与批量模式下的复选框行为不一致；改为绑定 `selected` 并补了单元用例锁定。
- 验证（真实服务 1440×900）：勾选 3 个文件后，行复选框 `checked: true`、
  面板显示「已选择 3 项 / 共 30.0 B」与三个名称、操作
  「下载全部 / 移动 / 全选 / 清空选择 / 删除全部」，单条详情字段隐藏；
  点「清空选择」后摘要消失；无控制台报错。
- 测试：`FileDetailsPanel` 新增 4 例（摘要内容、列表上限与剩余数量、批量事件、
  单条回退），`FileItem` 新增 1 例（复选框与选中态同步），前端 55 文件 / 373 用例通过。

### 3.5 审计日志（round 100，功能，用户请求）

- 需求：记录用户登录、上传、下载等重要操作，包含访问 IP、设备类型，用于追溯；
  **只读、不可删除、不可修改**。
- 实现：
  1. **迁移 0004**：`audit_logs` 表（时间、user_id + username 快照、action、result、
     target、ip、user_agent、device、detail）+ 三个索引；两个触发器
     `audit_logs_block_update` / `audit_logs_block_delete` 对任何 UPDATE/DELETE 抛错，
     从数据库层保证只追加；
  2. **领域**：`AuditLog` / `NewAuditLog`（builder）/ `AuditResult` / `AuditLogQuery` /
     `AuditLogPage`，以及 `AuditLogRepo`（只有 `append` / `list` / `distinct_actions`，
     刻意不提供修改与删除）；`describe_device()` 把 UA 归类为「平台 · 客户端」；
  3. **应用**：`AuditService`，写入「尽力而为」（失败只告警，不影响登录/上传等主流程）；
  4. **HTTP**：`crates/vfiles-http/src/audit.rs` 统一填 IP（`x-forwarded-for` 等）与 UA；
     在登录（成功/失败/限流）、上传（分片完成与直传）、下载（文件与目录打包）、
     分享下载、删除、移动、创建/停止分享、用户创建/更新/强制下线处埋点；
     `GET /api/audit/logs` 与 `/api/audit/actions` 仅管理员（复用 `require_admin`）；
  5. **前端**：新增 `/admin/audit` 页面（只读声明、关键字/动作/结果筛选、分页、
     时间/用户/动作/对象/结果/IP/设备列），账号菜单新增「审计日志」入口。
- **验证中发现并修复的两个真实问题**：
  1. `apiService.get(url, params)` 的第二参数就是查询参数，首版按 axios 习惯写成
     `{ params: {...} }`，导致筛选条件根本没发给服务端（页面永远显示全部记录）；
     浏览器验证当场暴露，随后补了断言「必须以扁平参数调用」的回归用例；
  2. 分片上传的完成接口最初把用户写死为空，上传记录显示「匿名」；
     改为取请求上下文（带用户名快照）后正确记为 `admin`。
- 验证：
  - **Rust**：仓储用例断言 `UPDATE`/`DELETE` 被触发器拒绝且原记录不变；
     筛选（关键字/IP/动作/结果/时间/分页）与 UA 解析（iPhone · Safari）；
     接口用例覆盖「登录/上传/下载被记录」「非管理员 403」「DELETE 返回 405」；
  - **浏览器**（1440×900，伪造 `x-forwarded-for` 与 Chrome UA）：审计页显示
     「共 7 条记录」，动作 下载/上传/登录成功/登录失败，用户 `admin`、
     IP `203.0.113.7`、设备「Windows · Chrome」、对象「审计.txt / audit-upload.txt」、
     结果「成功/失败」；按结果筛选「失败」后只剩 2 条登录失败记录；
     `DELETE`/`PUT /api/audit/logs` 均返回 405；无控制台错误（除刻意制造的 401/405）。
- 测试：Rust 新增 3 个仓储用例 + 1 个接口用例；前端新增 `AuditLogs` 6 例与
  服务层 2 例（扁平参数回归、空响应归一化），前端 56 文件 / 379 用例、19 个 Rust 目标全绿。

### 3.6 界面迭代（第 19 轮）：快捷键帮助面板（round 101，交互）

- 目标：主流网盘/文件管理器都有「? 查看快捷键」，VFiles 之前只在状态栏塞了一行提示，
  既看不全也无法发现（触屏/窄屏直接隐藏）。
- 改动：
  1. 新增 `KeyboardShortcutsDialog.vue`：分四组（导航 / 选择 / 操作 / 预览与搜索）列出
     29 个按键，用 `kbd` 呈现，并说明「输入框或弹窗聚焦时不生效」；
  2. 状态栏的提示文字改为按钮（图标 + 摘要 + 「全部快捷键（?）」），点击打开面板；
  3. 全局 <kbd>?</kbd> 打开/关闭面板（输入框内不劫持）；面板打开时纳入
     `anyOverlayOpen` 判定，避免快捷键穿透到列表操作；
  4. **Modal 组件统一支持 Esc 关闭**（此前只有少数弹窗自己处理），所有对话框受益。
- **浏览器验证中发现并修复的两个真实缺陷**：
  1. **详情面板压住状态栏**：`.desktop-details` 的 `max-height` 用视口高度
     （`100vh - navbar - 2rem` = 816px），而内容区只有 572px，面板溢出到状态栏上方
     26px 并**拦截了状态栏右侧的点击**（实测「全部快捷键」按钮点不到，
     `elementFromPoint` 返回 `desktop-details`）；改为 `max-height: 100%`（受 grid 行高约束）后，
     面板底边正好停在状态栏顶边（572px），按钮可点击。这个问题在详情面板打开时**一直存在**；
  2. 弹窗缺少统一的 Esc 关闭（见上）。
- 验证（真实服务 1440×900）：
  - `?` 打开面板（标题「键盘快捷键」、4 个分组、29 个按键标签），再按 `?` 关闭；
  - 点状态栏入口可打开（修复遮挡后），`Esc` 关闭；
  - 在搜索框内按 `?` 不会打开面板；
  - 无控制台报错。
- 测试：新增 `KeyboardShortcutsDialog` 3 例（分组与按键标签、常用条目文案、引导文案），
  `FileBrowser` 新增 3 例（`?` 开关、状态栏入口、输入框内不劫持），前端 58 文件 / 387 用例通过。

### 3.7 界面迭代（第 20 轮）：分享页筛选与批量清理（round 102，交互）

- 目标：链接多起来后需要按状态查看与清理，主流网盘都提供「有效/已过期」筛选与一键清理。
- 改动：
  1. 「我的分享」新增状态 chips（全部 / 有效 / 已过期 / 已被访问，带数量、互斥激活），
     列表按筛选渲染；筛选无结果时给出对应空状态 + 「查看全部」；
  2. 存在过期链接时页头出现「清理过期链接（N）」：二次确认后逐个停止并显示进度
     （`清理中 x/y`），完成后从列表移除并提示；部分失败会提示剩余数量。
- **浏览器验证中发现并修复的两个真实缺陷**（都影响既有功能）：
  1. `filesService.disableShare` 调用了 `/api/shares/{code}`，而分享路由挂在 `/api/share` 下：
     该请求命中**前端回退**并返回 200，看起来成功、服务端却没停用链接
     （即第 6 轮的「停止分享」与第 14 轮的「停止」按钮实际上都没生效）。
     改为 `/share/shares/{code}` 并补了断言 URL 的回归用例；
  2. 后端 `find_share_by_code` 会过滤过期分享（匿名访问必须如此），导致**所有者也无法停止过期链接**
     （DELETE 返回 404）——新增 `find_share_by_code_including_expired` 仅用于所有者操作，
     匿名访问仍严格校验过期时间；补了「过期链接可被所有者停止」的接口用例。
- 验证（真实服务 1280×900）：
  - chips `全部 3 / 有效 2 / 已过期 1 / 已被访问 1`；筛选「已过期」只剩过期项、「已被访问」只剩访问过的项、
    「有效」两项；
  - 点「清理过期链接（1）」→ 确认后 DELETE 返回 **204**，chips 变为 `全部 2 / 已过期 0`、
    按钮消失，且**接口复查**服务端只剩 2 个链接（过期项已真正删除）；无控制台报错。
- 测试：Rust 新增 1 个接口用例；前端 `SharedLinks` 新增 3 例（筛选与计数、筛选空态、
  批量清理）与 1 例 URL 回归，前端 58 文件 / 391 用例、19 个 Rust 目标与 clippy 全绿。

### 3.8 审计日志 CSV 导出（round 103，功能）

- 目标：合规/追溯场景需要把审计记录导出留档；主流审计界面都提供导出。
- 改动：
  1. 后端新增 `GET /api/audit/logs.csv`（仅管理员）：沿用列表的同一套筛选参数
     （关键字/动作/结果/时间），内部按 500 分页拉取、最多导出 10000 行；
     输出 `text/csv; charset=utf-8` + UTF-8 BOM（Excel 友好），
     `Content-Disposition` 附件名如 `audit-logs-20260921.csv`；
     字段做标准 CSV 转义（含逗号/引号/换行的值加引号）；
  2. **导出动作自身也写入审计**：新增动作 `audit.export`，说明里带上本次筛选条件，
     保证「谁在何时导出了哪些记录」可追溯；
  3. 前端审计页页头新增「导出 CSV」链接：自动携带当前筛选条件（`?keyword=…&result=…`），
     无记录时置灰；动作标签补充「导出审计日志」。
- 验证（真实服务 1280×900，Playwright 捕获真实下载）：
  - 文件名 `audit-logs-20260921.csv`，首字节为 BOM，表头
    `时间,用户,动作,对象,结果,IP,设备,说明`；
  - 含逗号的文件名被正确加引号：`"说明,含逗号.txt"`；
  - 按动作筛选后导出链接为 `/api/audit/logs.csv?action=file.download`，
    导出内容仅 1 条数据行且全部为该动作；
  - 导出后查询 `action=audit.export` 返回记录，说明为
    「导出审计日志 1 条（动作=file.download）」；无控制台报错。
- 测试：Rust 新增 1 个接口用例（CSV 头/内容、筛选生效、导出被审计、非管理员 403）；
  前端 `AuditLogs` 新增 2 例（导出链接携带筛选、无记录时置灰），前端 58 文件 / 393 用例通过。

### 3.9 审计日志时间范围筛选（round 104，交互）

- 目标：审计记录通常按时间窗口查阅，只有关键字/动作/结果筛选不够用。
- 改动：
  1. 审计页新增时间范围下拉：**全部时间 / 今天 / 近 7 天 / 近 30 天 / 自定义…**；
     今天与近 N 天按**本地时区**计算起点，自定义填写开始/结束日期（`until` 取次日 0 点，
     保证包含结束当天），范围显示在副标题上；
  2. 时间范围参与请求与导出链接（`?since=…&until=…`），「清除筛选」一并重置；
  3. **顺带加固**：`parse_timestamp` 兼容 SQLite `datetime('now')` 的
     `YYYY-MM-DD HH:MM:SS` 格式（按 UTC 解释）——此前若表里存在这种格式的记录（例如运维手工补写），
     整张审计表都会 500 读不出来；现在可以正常读取，并补了仓储用例。
- 验证（真实服务 1400×900，注入一条 10 天前、使用 SQLite 时间格式的记录）：
  - 全部时间 5 条（含 legacy）；**今天 4 条**（本地 0 点起，legacy 被排除）；
  - 近 7 天 4 条；近 30 天 5 条（legacy 回来）；自定义默认「近 7 天」4 条；
    自定义「今天→今天」4 条；
  - 导出链接带上范围：`/api/audit/logs.csv?since=2026-09-21T16%3A00%3A00.000Z&until=…`；
  - 无控制台报错（注入前该页会因 500 显示「加载失败」，现已正常）。
- 测试：Rust 新增 1 个仓储用例（SQLite 时间格式兼容）；前端 `AuditLogs` 新增 2 例
  （时间范围预设与导出链接、自定义范围含结束当天），前端 58 文件 / 395 用例、
  19 个 Rust 目标与 clippy 全绿。

### 3.10 界面迭代（第 23 轮）：移动端搜索头部吸顶与 chips 单行滚动（round 105，交互）

- 目标：移动端搜索结果页整页滚动，搜索框与筛选会随之上移消失；类型较多时 chips 换行挤压列表。
- 改动：
  1. `.mobile-search-toolbar` 改为 `position: sticky`（吸附在顶栏下方 + 安全区），
     滚动结果时搜索框、摘要与 chips 始终可见；
  2. `SearchResultToolbar` 增加窄屏（≤700px）样式：整体改为纵向堆叠，
     chips 行 `flex-wrap: nowrap` + `overflow-x: auto`（隐藏滚动条、chip 不收缩），
     单行横向滚动。
- 验证（真实服务，iPhone 13 视口 390×844，7 种类型）：
  - chips 为 `全部 8 / 文件夹 1 / 文档 3 / 图片 1 / 视频 1 / 音频 1 / 其它 1`，
    `scrollWidth > clientWidth`（**可横向滚动**）且 `flex-wrap: nowrap`；
  - 搜索栏 `position: sticky`，滚动后仍停在顶栏下方（y=13，摘要与 chips 紧随其下）；
  - 把 chips 滚到最右并点击「其它」→ 摘要变为「找到 8 项 · 其它 1 项」、结果 1 行；
  - 无横向页面溢出、无控制台报错。
- 测试：既有 `SearchResultToolbar` / `FileBrowser` 用例保持通过（本轮为纯样式改动，
  行为由浏览器验证覆盖），前端 58 文件 / 395 用例通过。

### 3.11 界面迭代（第 24 轮）：键盘导航增强（round 106，交互）

- 目标：列表已有 ↑↓/Home/End/Enter/Esc，但缺少主流文件管理器的翻页、按键定位与空格选择。
- 改动：
  1. **PageUp / PageDown**：按整屏移动，步长由实际行高与列表可视高度计算（无布局环境兜底 10 行）；
  2. **按键定位（type-ahead）**：可打印字符（字母/数字/中文等）累积成前缀并在 800ms 后重置，
     从当前位置向后查找（绕回开头）名称以该前缀开头的条目并滚动到可见；
     底部状态栏显示「定位：<前缀>」，**无匹配时清空前缀且不拦截按键**，<kbd>Esc</kbd> 取消；
  3. **空格**：切换当前高亮行的选中态（并自动进入批量模式，与勾选复选框一致）；
  4. 快捷键面板同步补充 PageUp/PageDown、按键定位与空格。
- **浏览器验证中发现并修复的问题**：`Esc` 前缀清理最初写在处理函数中段，被更早的
  「逐层退出（高级搜索 → 预览 → 批量 → 选择）」分支 `return` 掉，导致前缀清不掉；
  合并进最早的 Esc 分支后正常。
- 验证（真实服务 1440×900，41 个文件）：
  - PageDown：`文件-11` → `文件-22`（一屏 11 行），PageUp 回到 `文件-11`；
  - 按键定位：按 `b` 跳到 `beta.txt` 且显示「定位：b」，再按 `e` 显示「定位：be」，
    按 `Esc` 后提示消失；按无匹配的 `z` 不跳转也不显示提示；
  - 空格：高亮行选中（已选 1 项）→ 再按取消（已选 0 项）；
  - 列表滚动跟随高亮行，全程无控制台报错。
- 测试：`FileBrowser` 新增 3 例（按键定位与前缀显示/清除、无匹配不拦截、翻页与空格选择），
  前端 58 文件 / 398 用例通过。

### 3.12 界面迭代（第 25 轮）：加载骨架统一（round 107，视觉）

- 目标：加载态此前各写一套——文件列表是骨架，侧栏是三行占位，预览/版本历史/移动对话框用
  spinner，三个管理页各写一份骨架 CSS（含各自的 shimmer 动画与尺寸）。
- 改动：
  1. 新增通用组件 `components/common/SkeletonList.vue`：统一微光动画（`--vf-skeleton-base/shine`）、
     `rows` 行数与三种变体（`rows` 列表行 / `folders` 目录行 / `lines` 文本行），
     根节点带 `role="status"`、`aria-busy`、可读标签，并遵循 `prefers-reduced-motion`；
  2. 替换四处自写骨架（侧栏概览、用户管理、我的分享、审计日志）为共享组件，
     删除各自重复的 CSS 与 `@keyframes`；
  3. **结构性内容一律用骨架而不是 spinner**：预览对话框（文本 10 行 / 图片 3 行卡片）、
     版本历史列表（5 行）与 diff（8 行文本行）、移动对话框目录列表（4 行目录行）。
- 验证（真实服务 1400×900；对 overview/audit/admin/shares/history/content/tree
  接口注入延迟以捕获加载态）：
  - 侧栏「加载工作区概览」3 行（lines）；审计「加载审计日志」4 行；分享「加载分享链接」3 行；
    用户管理「加载用户列表」4 行；
  - 预览「加载预览 说明.md」10 行（lines）；版本历史「加载历史记录」5 行（rows）；
    移动对话框「加载目录」4 行（folders，见截图）；
  - 列表首屏仍为 `FileSkeleton`（跟随列表/网格模式）；全程无控制台报错。
- 测试：新增 `SkeletonList` 3 例（行数与无障碍属性、变体差异、默认值），
  并把 `FilePreviewModal` 的加载态断言从「加载预览中...」改为骨架断言；
  前端 59 文件 / 401 用例通过。

### 3.13 界面迭代（第 26 轮）：网格视图键盘导航校正（round 108，交互）

- 背景：第 24 轮加入的 PageUp/PageDown 步长只按**列表行**计算（`tr/file-item`），
  在网格视图下拿不到行元素，只能兜底 10——3 列网格里「一屏」其实只有约 3 行，
  步长偏小；需要按布局把列数算进去。
- 改动：`pageStep()` 改为按视图分派：
  - 列表：可视行数 = 容器高度 ÷ 行间距（原有行为）；
  - **网格**：列数由 `columnsFromElements` 推断，行高取同列相邻两行的间距
    （只有一行时退回卡片高度 + 间距），步长 = 可视行数 × 列数。
- 验证（真实服务 1400×900，31 个条目）：
  - 3 列网格（隐藏详情面板后）：`←→` 按 1 张卡片（0→1→0）；
    **`↓` 按整行**（0→3），`↑` 回到 0；`PageDown` 按 2 可视行 × 3 列 = **6**（0→6）；
    按键定位 `z` 跳到 `zebra.txt`（第 31 项）且列表滚动到可见；全程无控制台报错；
  - 单列网格（打开目录树与详情面板时列表列仅 361px，`auto-fill minmax(194px,1fr)` 只能排 1 列）：
    `↓` 按 1 项、`PageDown` 按 10 项（兜底），行为与列表一致。
- 测试：`FileBrowser` 新增 1 例（在 jsdom 中构造 3 列 / 行高 200 / 可视 600 的布局，
  断言 `↓` 走 3 项、`PageDown` 走 9 项），前端 59 文件 / 402 用例通过。

### 3.14 界面迭代（第 27 轮）：通知中心（round 109，交互）

- 目标：toast 3~5 秒后自动消失，错过的提示（例如批量操作里的失败项）就再也看不到；
  主流应用都会保留「通知中心」入口。
- 改动：
  1. `app` store 的提示增加 `createdAt`，并新增 `notificationHistory`（保留最近 50 条）、
     `unreadNotifications`、`markNotificationsRead()`、`clearNotificationHistory()`；
  2. 新增 `NotificationCenter.vue`：顶栏铃铛 + 未读角标 + 下拉面板（类型图标、
     消息、相对时间、「清空」），打开即清零未读，支持点击外部与 Esc 关闭；
  3. **提示条位置调整**：桌面端从右上角移到**右下角**（原来会盖住通知中心与账号菜单），
     移动端保持贴底但抬到底部操作栏之上。
- 验证（真实服务 1400×900 与 iPhone 13 视口）：
  - 登录后角标 1，连续收藏/取消收藏后角标 3；打开面板角标清零；
  - 面板按时间倒序列出「已取消收藏 / 已加入收藏 / 登录成功」（含类型颜色与「今天 01:44」）；
  - **toast 全部消失后（toasts: 0）历史仍为 3 条**；点「清空」→「暂无通知」；Esc 关闭；
  - 桌面 toast 位于右下（top 834）与右上通知面板（47–137）**不重叠**；
    移动端 toast 底边 586 ≤ 底部操作栏顶边 608；无控制台报错。
- 测试：新增 `NotificationCenter` 5 例（角标与清零、历史倒序与相对时间、
  toast 消失后历史仍在、清空与空态、Esc/外部点击关闭），前端 60 文件 / 407 用例通过。

### 3.15 审计日志概览聚合（round 110，功能）

- 目标：审计页此前只有明细列表，看「谁操作最多 / 哪些动作最频繁 / 失败多少」需要自己数。
- 后端：
  1. 领域层新增 `AuditCount` 与 `AuditLogSummary { total, failures, users, actions }`，
     仓储新增 `summarize(&query, top)`；
  2. SQLite 用三条聚合查询（总量 + 失败数用 `SUM(CASE WHEN result='failure')` 一次取出；
     用户 / 动作分别 `GROUP BY` 后按次数倒序取 Top N），过滤条件与列表完全一致，
     空用户名归入 `(匿名)`；
  3. 新增 `GET /api/audit/summary`（仅管理员，复用 `require_admin`）。
- 前端：审计页新增**概览条**——「共 N 条 / 失败 M 条」+ Top 用户与 Top 动作 chips
  （带次数，动作显示中文标签）；点用户 chip 填入关键字筛选、点动作 chip 设置动作筛选、
  点失败数只看失败；概览与列表用同一组筛选一起刷新（概览失败不影响列表）。
- 验证（真实服务 1400×900，制造 admin/alice 的登录、下载、创建分享与失败登录）：
  - 概览条显示 `共 17 条 · 失败 3 条 · 用户 admin 11 / alice 6 · 动作 登录成功 8 / 下载 3 /
    登录失败 3 / 创建分享 3`；
  - 点 alice chip → 列表 6 条且概览更新为 `alice 6`、动作「登录失败 3 / 登录成功 3」；
  - 点「失败 3 条」→ 列表 3 条、动作只剩「登录失败 3」；
  - 接口 `/api/audit/summary` 返回与界面一致的聚合；无产品级控制台报错。
- 测试：Rust 新增 1 个仓储用例（总量/失败/Top 用户与动作、匿名归并、筛选与 Top 上限）
  与 1 个接口用例（聚合内容、筛选、非管理员 403）；前端 `AuditLogs` 新增 2 例
  （概览条与 chip 筛选、概览失败不影响列表），前端 60 文件 / 409 用例、19 个 Rust 目标全绿。

### 3.16 转移所有权（round 111，功能，用户需求）

- 需求：把目录或文件转移给另一个用户，**最好连历史版本一起转过去**。
- 设计：条目属于某个命名空间（`entries.namespace_id`），版本历史（`entry_versions`）按
  **条目 ID** 关联、blob 是全局内容寻址存储——因此「转移所有权」只需把条目的
  `namespace_id` 改为目标用户的命名空间，**历史版本天然随行、无需复制数据**。
- 实现：
  1. `EntryRepo::transfer_entries(&[(EntryId, NamespaceId)])`（单事务，唯一约束冲突 →
     `PathConflict`）；`UserRepo::list_transfer_targets`（启用中、排除自己）；
  2. 新增应用服务 `OwnershipService`：校验目标用户与自转、源路径重叠、**目标同名路径冲突**，
     用 `find_subtree` 展开目录子树，为缺失的**祖先目录**在目标命名空间补齐
     （只补齐不属于本次转移集合的祖先），最后转移并在**双方各写一条快照**
     （「转移给 X：备注」/「接收来自 Y：备注」）；
  3. 接口：`POST /api/files/transfer`、`GET /api/files/users/directory`，并写入审计
     （新动作 `file.transfer`）；
  4. 前端：`filesService.transferOwnership/listTransferTargets`；
     新增 `TransferOwnershipDialog.vue`（用户搜索选择、备注、二次确认、加载骨架与空态）；
     入口：右键菜单、详情面板操作区、批量操作条；成功后刷新列表、清空选择并提示。
- **联调中发现并修复的两个真实缺陷**：
  1. **嵌套确认框被盖住**：从对话框里调用全局 `confirmDialog` 时两者 z-index 都是 100，
     而 `DialogHost` 在 DOM 中更靠前，导致确认按钮被下层对话框遮挡、点不到。
     给 `Modal` 增加 `layer="overlay"`（z-index 1000）并让 `DialogHost` 使用；
  2. **祖先目录冲突**：最初为每个被转移条目的父目录都调用 `ensure_directory_path`，
     于是「转移目录本身」时先在目标创建同名目录，随后转移该目录又撞上唯一约束 → 409。
     改为只补齐**不属于本次转移集合**的祖先目录。
- 验证（真实服务 1400×900，admin 把 `交接资料/`（含 `图纸/平面图.txt`、`说明.txt`）转给 alice）：
  - 界面：右键菜单出现「转移所有权」；对话框显示待转条目与目标用户并注明「版本历史会一起转移」；
    确认后提示「已把 **4** 个条目转移给 alice」；
  - 源侧根目录只剩 `个人笔记.txt`；目标侧出现 `交接资料`，进入后可见 `图纸` 与 `说明.txt`；
  - **历史随行**：alice 查 `GET /api/history?path=交接资料/说明.txt` → 1 个提交
    「导入: 交接资料/说明.txt」（原创建者是 admin）；
  - 数据核对：4 个条目归属 alice，`entry_versions` 随条目保留；
    admin 侧快照「转移给 alice：项目交接给 alice」、alice 侧「接收来自 admin 的文件：项目交接给 alice」；
  - 审计记录：「转移给 alice（4 个条目，含版本历史）」；
  - **无越权**：admin 再读该文件内容/目录返回 404；历史接口只返回 admin 自己命名空间里的历史快照记录；
  - 无控制台报错。
- 测试：Rust 新增 2 个接口用例（目标可见 + **历史完整**、自转 400、冲突 409、未登录 401、
  目标列表不含自己）；前端新增 `TransferOwnershipDialog` 6 例与 `FileBrowser` 1 例；
  前端 61 文件 / 416 用例、19 个 Rust 目标与 clippy 全绿。

### 3.17 访问令牌（round 112，功能，用户需求）

- 需求：给 CLI、构建系统等程序提供上传/下载用的凭证（无需登录会话）。
- 实现：
  1. 迁移 `0005_access_tokens.sql`：`access_tokens(id, user_id, name, token_hash, token_prefix,
     scopes, expires_at, last_used_at, revoked_at, created_at)` + 用户/摘要索引；
  2. 领域：`AccessToken`（`is_active(now)`）、`NewAccessToken`、`AccessTokenRepo`
     （create/list_for_user/find_by_hash/touch_last_used/revoke），SQLite 实现只存 **SHA-256 摘要**；
  3. 应用：`AccessTokenService`——明文 `vfat_<64 hex>`（两个 UUIDv4 拼接，256 位随机），
     创建校验名称与有效期白名单（0/30/90/365 天），鉴权校验未撤销/未过期并刷新 `last_used_at`；
  4. HTTP：中间件把 `Authorization: Bearer vfat_…` 归一化成内部 cookie，
     `optional_auth_user` 增加令牌分支、`require_auth_user` 复用它——**所有既有接口无需改动即支持令牌**；
     `POST/GET /api/tokens`、`DELETE /api/tokens/{id}`、`GET /api/tokens/expiry-options`，
     且令牌管理接口**只接受会话鉴权**；审计动作 `token.create`/`token.revoke`；
  5. 前端：账号菜单 →「访问令牌」页（列表 + 状态徽标 + 一次性明文对话框 + 复制 + 撤销二次确认 +
     可复制的 curl 用法示例）。
- **浏览器验证中发现并修复的缺陷**：令牌 DTO 的时间戳原本用 `OffsetDateTime::to_string()`
  （`2026-09-21 18:17:32 … +00:00:00`），不是 RFC3339，前端 `new Date()` 解析失败导致
  「创建时间/最近使用/有效期」直接显示原始字符串；改用统一的 `format_timestamp`（RFC3339）后正常。
- 验证（真实服务 1400×900 + Bearer-only 请求）：
  - 界面创建「CI 构建」（90 天）→ 明文 `vfat_4863464d…`（64 hex），列表显示前缀、`有效`、
    `从未使用` → 使用后变「今天 02:17」、有效期「至 2026/12/21」；
  - **不用 Cookie、只用令牌**：`upload/init → chunks → complete` 全部 200，
    `GET /api/files/tree?path=ci` 返回 `["ci"]`，下载内容与上传一致（`hello build`）；
  - **匿名（无 Cookie 无令牌）**：401；**撤销后**再用令牌：401；**用令牌创建令牌**：403；
  - 审计日志新增 `token.create` / `token.revoke` 记录。
- 测试：Rust 新增 1 个端到端接口用例（创建 → Bearer 上传/下载 → 列表含前缀不含明文 →
  记录 last_used_at → 令牌不能创建令牌 403 → 撤销后 401 → 非法有效期 400 → 匿名 401）；
  前端新增 `AccessTokens` 7 例；前端 62 文件 / 423 用例、19 个 Rust 目标与 clippy 全绿。

### 3.18 迭代收尾（round 113）：候选项收口

本轮（自第 1 轮起）界面迭代按用户要求收尾，最后完成三项候选：

1. **通知中心增强**：面板顶部新增「全部 / 成功 / 失败 / 警告 / 信息」类型标签（互斥激活，
   无记录时给出「没有 X 通知」空态）；每条支持**单条移除**（hover/键盘聚焦时显示按钮，
   `app.removeNotificationFromHistory` 同时清掉同名 toast）；新增 2 个用例。
2. **网格缩略图加载体验**：缩略图未就绪时铺骨架占位，`load` 后**淡入**（`opacity` 过渡，
   遵循 `prefers-reduced-motion`），并补 `fetchpriority="low"`；命中缓存时通过
   `img.complete` 兜底补 `is-loaded`，避免缓存命中导致图片一直透明。
   验证：真实 PNG 缩略图 `naturalWidth=288`、`is-loaded`、opacity 1、占位元素消失、
   `loading=lazy` + `decoding=async` ✓，无控制台报错。
3. **审计导出附带汇总**：CSV 开头追加汇总区块（`汇总,<筛选说明>` / `总记录,N` / `失败,N` /
   `Top 用户,key count；` / `Top 动作,…`），随后是原表头与明细，便于直接归档；
   浏览器实测首 8 行为汇总 + 空行 + 表头 + 数据行 ✓。

验证：浏览器实测（1400×900）通知筛选「失败」→「没有失败通知」、成功筛选 1 条、单条移除
后 1→0；CSV 汇总区块与筛选一致。前端 62 文件 / 425 用例、19 个 Rust 目标与 clippy 全绿。

### 3.19 单请求上传接口（round 114，功能，用户需求）

- 需求：现有 init/分片流程很难用在 `curl` 上，希望有对 curl 友好的上传接口。
- 实现：
  1. 新增 `PUT /api/files/upload`（query：`path` / `filename` / `message`）与
     `PUT /api/files/upload/{*path}`（路径写在 URL 里，配合 `curl -T` 会自动补文件名）；
     请求体为**原始文件内容**，流式落盘；
  2. 抽公共收尾逻辑 `finish_single_upload`，与既有 multipart（`POST /api/files/upload`）
     共用 `init_upload` + `complete_upload_from_stream`，因此版本历史/快照/审计行为一致；
  3. 边写边校验大小，`Content-Length` 已知时提前 413；目标解析支持
     「URL 路径整段当目标」与「`path` 最后一段当文件名」，目标是已存在目录时返回 400 明确提示。
- **顺带修复的真实缺陷**：配置项 `VFILES_MAX_FILE_SIZE_MB`（界面上展示的单文件上限）
  此前只在客户端预检，**服务端并未强制**，任何 API 都能上传超限文件；
  现在所有上传路径（multipart / 分片 init / 原始 body）都取
  `min(上传硬上限, 单文件配置上限)` 校验，超限返回 413（分片 init 沿用既有 400+文案）。
- 验证（真实 `curl`，含访问令牌）：
  - `curl -T app-1.0.tar.gz "…/upload/ci/app-1.0.tar.gz?message=构建产物"` → 200，
    2MB 文件下载后 **sha256 与上传一致**；
  - `curl -X PUT --data-binary @说明.txt "…/upload?path=ci&filename=说明.txt"` → 200；
  - `PUT …/upload/ci/url-form.txt` → 列表可见、内容一致；
  - 1MB 上限实例上：2MB 直接 PUT → **413** `FILE_TOO_LARGE`（`limit_bytes/size_bytes`）；
    `Transfer-Encoding: chunked`（无 Content-Length）→ 累计超限即 **413**；1MB 内 → 200；
  - 缺文件名 → 400、匿名 → 401。
- 测试：Rust 新增 2 个接口用例（原始 body 上传含 URL 形式/覆盖成新版本/目录目标 400/匿名 401；
  所有上传路径按配置上限拒绝）；前端令牌页示例改为单请求上传；前端 62 文件 / 425 用例、
  19 个 Rust 目标与 clippy 全绿。

### 3.20 页头操作一致性（round 115，视觉）

- 反馈：「用户管理」界面缺少「我的分享」那样的刷新、返回按钮，一致性差。
- 现状核对：`用户管理`/`我的分享`/`审计日志`/`访问令牌` 四个页面的页头本应是同一套语言，
  实测只有用户管理缺少**返回文件**（刷新已有，且带 loading 图标旋转）。
- 改动：`AdminUsers.vue` 页头补上「返回文件」按钮（`IconFolderOpen` + `router.push("/")`），
  与其余三个页面完全一致；同步在用户指南页头说明里点明。
- 验证：新增 2 个前端用例（页头动作同时包含「刷新」「返回文件」；点「刷新」会重新拉取用户列表），
  前端 62 文件 / 427 用例通过。

### 3.21 管理员账号恢复（round 116，功能+修复）

- 需求：忘记管理员账号/密码时如何从后台查询与恢复。
- 新增：`vfiles user reset-password`
  （`--username` / `--user-id` 定位，`--password` / `--password-hash` / `--generate` 提供新密码，
  默认撤销该用户全部会话，`--keep-sessions` 可保留）；会先打印账号信息（id/用户名/邮箱/角色/状态）
  便于确认。`vfiles user update` 新增 `--username`（改名）。
- **顺带修复的两个真实缺陷**（用非法用户名建首个管理员时暴露）：
  1. **bootstrap 未校验用户名**：`vfiles user create --username root-admin` 会把含 `-` 的
     用户名直接写入库；随后 `user_from_row` 解析失败 → `user list`、按名查找、登录全部报
     `Internal error: Invalid username`，实例实际不可用。现在 bootstrap 与普通创建一样校验；
  2. **读取不宽容**：历史行会让整表读取失败。新增 `Username::from_stored`（读取不校验），
     使这类账号能被列出、重置密码与改名修复。
- 验证（真实 CLI + 服务端鉴权）：
  - 非法用户名建首个管理员 → 现在直接报 `Invalid username: Username contains invalid characters`；
  - 注入历史坏账号后 `user list` 正常列出；`reset-password --username root-admin --password …`
    成功重置并撤销会话；`user update <id> --username recoveredadmin` 修复后
    **新密码登录 200**、错误密码 401；既有管理员原密码仍 200；
  - `reset-password` 参数校验：密码过短 / 用户不存在 / 未提供新密码均给出明确错误。
- 测试：`vfiles-infra-sqlite` 新增 1 个用例（历史非法用户名可列出、可宽容查找、可改名）；
  19 个 Rust 目标与 clippy 全绿。文档新增 `docs/ADMIN_SETUP.md`「忘记管理员账号 / 密码」一节。

### 3.22 删除二次确认（round 117，交互修复，用户反馈）

- 反馈：删除文件/目录时没有确认对话框，容易误操作。
- 现状核查：多选批量删除（`useFileSelection.batchDelete`）与 Delete 键走批量路径**已有**确认；
  但**单条删除**（右键菜单 / 详情面板 / 移动端长按菜单 / Delete 键在仅高亮未选中时）走的
  `handleDelete` **没有确认**，点击即删。
- 后端事实：`entry_versions.entry_id ON DELETE CASCADE`，删除条目会级联删掉其全部版本历史，
  blob 引用随之释放 → **不可恢复**，确认文案必须写明。
- 改动：`handleDelete` 增加 `confirmDialog`（danger）——文件：「确定要删除“X”吗？删除后不可撤销。」；
  目录：「…目录及其全部内容将被删除，且不可撤销。」；确认按钮红色「删除」，拒绝则不发出请求。
- 验证（真实服务 1400×900，DOM 断言 + 截图）：
  - 右键文件 → 删除 → 弹框（标题「删除文件」、文案含文件名与「不可撤销」、红色「删除」按钮）；
    点「取消」→ 文件仍在；
  - 右键目录 → 删除 → 文案为「目录及其全部内容将被删除，且不可撤销。」；取消 → 目录仍在；
  - 详情面板「删除」按钮 → 同一确认框；拒绝 → 无删除请求；
  - 确认删除 → toast「文件删除成功」，UI 列表与服务端 `/api/files/tree` 均不再含该文件；
  - 全程无控制台报错。
- 测试：`FileBrowser` 新增 3 例（单条删除需确认且拒绝不发请求/接受才删除、目录文案含
  「全部内容」与「不可撤销」、详情面板入口同样确认）；前端 62 文件 / 430 用例通过。

### 3.23 目录树溢出修复（round 118，交互修复，用户反馈）

- 反馈：目录比较多时「全部文件」目录树会盖到状态栏上，部分内容无法显示，也无法滚动。
- 复现（1440×900，30 个目录）实测：aside 有界 572px，但 `.directory-tree` 高 961px
  （按内容撑开）→ 溢出 aside 底部 388px **盖住状态栏**；树的滚动容器
  `scrollHeight == clientHeight == 961`（没被裁剪 → 不产生滚动条 → **无法滚动**），
  概览整块被顶到状态栏以下（**无法显示**）。
- 根因：`nav.directory-tree` 是 flex 子项且未设 `min-height`，其自动最小高度 = 内容最小尺寸，
  目录多时无法收缩、按内容撑高；`.directory-tree-scroll`（`overflow-y: auto`）的父级高度不受限
  → 内部滚动永不触发。列表列当初就是用同一手法（`minmax(0, 1fr)` 行 + `min-height: 0`）修好的。
- 改动：
  1. `.directory-tree`：`flex: 1 1 auto` + `min-height: 10rem`（覆盖自动最小值并给可用地板）
     + `overflow: hidden`；
  2. `.browser-sidebar`：`overflow: hidden` 防御（任何意外内容都在侧栏内裁剪）；
  3. `.sidebar-overview`：`min-height: 0` + `max-height: calc(100% - 10rem)` + `overflow-y: auto`
     —— 首版修复后矮窗口（1280×600）实测发现树被挤成 0px、概览 371px 反而溢出；
     概览上限后两者按「树 ≥10rem，概览剩余且自身滚动」分配。
- 验证（真实服务，30 个目录 + 全部 30 个子目录展开 = 60 行，scrollH 1872）：
  | 视口 | 树 | 概览 | 滚动 | 末行「子目录30」可达 | 盖状态栏 |
  | --- | --- | --- | --- | --- | --- |
  | 1440×900 | 201px（有界） | 371px 自然高、齐状态栏顶 | scrollH 1872 > 201 ✓ | ✓ | 否 |
  | 1280×600 | 160px（10rem 地板） | 112px（上限，自身滚动） | scrollH 1872 > 160 ✓ | ✓ | 否 |
  两个视口均无控制台报错；概览块均完整可见（`overviewFits: true`）。
- 测试：CSS 布局修复（浏览器实测覆盖），前端 62 文件 / 430 用例保持全绿。

### 3.24 视图/排序弹层定位修复（round 119，交互修复，用户反馈）

- 反馈：视图按钮弹出的面板左边被挡住一部分，弹出起始位置不正确。
- 复现实测（面板 = `.dropdown-menu`，卡片容器 = `.file-browser-box`，`overflow: hidden`）：
  - 桌面 1440×900：面板 12..242，容器左缘 80 → **左侧 68px 被裁掉**；
  - 移动 390×844：面板 3..233，容器左缘 32 → **左侧 29px 被裁掉**。
- 根因：两个弹层（`ViewOptions` 视图、`SortMenu` 排序）都用 Bulma 的
  `dropdown is-right`（右缘对齐触发器、**向左展开**）；而「视图」按钮位于命令栏**左侧**，
  面板（230px）向左越过 `.file-browser-box` 的左缘被 `overflow: hidden` 裁掉。
- 改动（CSS，双组件一致）：
  1. **桌面**：改为左缘对齐触发器、向右展开（`left: 0; right: auto`）；
  2. **窄屏（移动端搜索行）**：以 `.mobile-search-row` 为定位容器（`position: relative` +
     弹层根 `position: static`），面板 `width: fit-content; max-width: 100%; margin-inline: auto`
     在行宽内水平居中——无论按钮在行内什么位置都不会被裁；
  3. 顺带修复 `MobileSearchBar` 里两条 scoped 但**匹配不到子组件元素**的样式
     （z-index 与按钮高度）：补 `:deep()` 让它们真正生效。
- 验证（真实浏览器 8 组）：
  | 视口 | 视画面板 | 排序面板 | 结果 |
  | --- | --- | --- | --- |
  | 1440×900 | 172..402 | 247..439 | 均在容器 80..1360 内 ✓ |
  | 1280×800 | 172..402 | 247..439 | 均在容器 80..1200 内 ✓ |
  | 390×844 | 80..310 | 99..291 | 均在容器 32..358 内 ✓ |
  | 320×640 | 54..266 | 64..256 | 均在容器 32..288 内 ✓ |
  8 组全部满足：面板完整落在卡片容器内、不超出视口、左缘未被其它元素遮挡（修复前 2 组被裁）。
- 测试：CSS 布局修复（浏览器实测 4 视口 × 2 面板），前端 62 文件 / 430 用例保持全绿。

## 3. 后续迭代计划（按优先级）

### 3.1 静态资源预压缩（性能，高）

- `[x]` 构建期生成 `.br`/`.gz`，服务端按 `Accept-Encoding` 直接返回（round 12）。
- `[x]` 评估结论：`index.html` 已超阈值并被预压缩，dist 内无二进制资源、其余压缩格式
  二次压缩收益 0–2%，**决定不扩大预压缩范围**（round 55，见 §2.62）。

### 3.1c 删除/移动路径的子树遍历（性能，中）

- `[x]` `EntryRepo::find_subtree` 单次范围查询取代按目录递归（round 20）。
- `[x]` 移动冲突检查已改为单次 `find_paths` 批量查询 + 单事务 `move_entries`
  （round 22 落地，round 55 复核确认无逐后代查询残留，见 §2.62）。

### 3.1b 服务端目录分页（性能，高）

- `[x]` 新增 `GET /api/files/list[/{path}]`，支持 `limit`/`offset`/`commit` 与
  `total`/`has_more`，客户端按需加载（round 30）。
- `[x]` 搜索结果分页：`/api/files/search` 返回 `items/limit/offset/has_more`，
  客户端滚动按需加载（round 41，见 §2.48）。
- `[x]` 搜索结果分页正确性：分页统一收敛到「按得分排序之后」，修复两路命中各自
  `OFFSET` 导致的翻页重复/遗漏（round 53，见 §2.60）。
- `[x]` 目录分页下沉到 SQL（`LIMIT/OFFSET` + `COUNT`），5 万条目目录从 480ms 降到
  40ms（round 60，见 §2.67）。
- `[~]` 游标（cursor）替代 `offset`：实测残余 OFFSET 成本仅 40→70ms（5 万条目），
  收益有限，评估后不做；搜索评分下沉到 SQL 仍是更大的独立课题。

### 3.1e 移动路径的批量校验与事务（性能，中）

- `[x]` 批量路径存在性检查 + 事务内批量更新（round 22）。

### 3.1d 快照保留与 blob 回收（稳定性，中）

- `[x]` 孤儿 blob 文件 GC（round 27）：清理「有文件、无元数据行、无引用」的残留。
- `[x]` 快照保留策略（round 28）：`prune-snapshots --keep N` 裁剪旧快照并释放其
  blob 引用，配合 `gc-blobs` 回收磁盘。
- `[x]` 在服务内按周期自动执行维护任务（round 39，见 §2.46）。
- `[x]` 时间窗口保留：`--older-than-days` / `VFILES_MAINTENANCE_SNAPSHOT_MAX_AGE_DAYS`，
  与数量策略取「与」（round 55，见 §2.62）。

### 3.2 缩略图格式与容量（性能 + 稳定性，中）

- `[x]` 容量上限 + 按 mtime 回收，日志可观测（round 6）。
- `[x]` 按总字节数（而非仅条目数）设限，上限可用环境变量覆盖；新增 TIFF/ICO/QOI
  解码支持（round 40，见 §2.47）。
- `[x]` AVIF 输出（默认关闭，`VFILES_THUMBNAIL_AVIF`）与 Accept 协商（round 54，见 §2.61）。
- `[~]` AVIF **解码**：需要 `avif-native`（libdav1d，C 依赖），会引入系统库与
  交叉编译成本，评估后暂不做——需要时可通过把 AVIF 源文件先转码再入库规避。
- `[x]` 按需 WebP：实测无损 WebP 在照片类内容上是 JPEG 的 5.6 倍，**决定不提供**。

### 3.3 `FileBrowser.vue` 拆分（稳定性，中）

- `[x]` 下载队列与文件预览抽为 composable（round 7，3288 → 2858 行）。
- `[x]` 搜索抽为 `useFileSearch`，下载队列面板抽为 `DownloadQueuePanel.vue`
  （round 8，2858 → 2625 行）。
- `[x]` 目录管理与移动/路径计算抽为 `useDirectoryManager` + `utils/filePaths`
  （round 9，2625 → 2430 行）。
- `[x]` 批量选择/批量操作抽为 `useFileSelection`（round 10，2430 → 2268 行）。
- `[x]` 移动对话框抽为 `useMoveDialog`（round 15，2362 → 2313 行）。
- `[x]` 移动端手势抽为 `useTouchGestures`（round 17，2331 → 2249 行）。
- `[x]` 详细信息面板抽为 `FileDetailsPanel.vue`、批量操作条抽为
  `BatchActionBar.vue`（round 37，2788 → 2516 行，见 §2.44）。
- `[x]` 预览弹窗抽为 `FilePreviewModal.vue`（round 38，2516 → 2277 行，见 §2.45）。
- `[x]` 搜索框与高级筛选抽为 `BrowserSearchBox.vue`（round 39，2277 → 2031 行，见 §2.46）。
- `[x]` 抽出移动端搜索行（`MobileSearchBar.vue`）与工具栏动作组
  （`DesktopCommandBar.vue`）：2484 → 2301 行（round 62，见 §2.69）。
- `[~]` 目标 < 1500 行：评估后不追求——剩余 ~800 行主要是编排逻辑与内容区，
  外移模板只能到 ~1600 行，继续缩小需要把编排搬进组合式函数，风险与收益不成比例。
- 验收：单文件行数持续下降、已有测试保持通过（拆分后 265 个用例全绿）。

### 3.4 交互增强（中，对齐主流云盘）

- `[x]` 列表视图点击表头排序、`Ctrl/⌘+A`、`Delete`/`F2`/`Enter`/`Esc`（round 4）。
- `[x]` 右键上下文菜单；Shift 范围选择 / Ctrl(⌘) 加选（round 5）。
- `[x]` 面包屑可点击跳转 + 当前目录子文件夹下拉（round 11）。
- `[x]` 预览内上一个/下一个（按钮 + ←/→ 方向键）与位置指示（round 14）。
- `[x]` 拖放移动：拖到目录行/卡片或面包屑路径段（round 23）。
- `[x]` 加载骨架屏（列表/网格）取代单一 spinner（round 24）。
- `[x]` 移动端长按呼出上下文菜单（round 29）。
- `[x]` 排序入口提为工具栏控件，网格与移动端一致可用（round 44，见 §2.51）。
- `[x]` 空状态与加载骨架屏已统一（桌面/移动共用 `.browser-empty` 与 `FileSkeleton`，
  进度条共用 `ProgressBar`），round 58 复核确认，见 §2.65。

### 3.5 稳定性与可观测性（中）

- `[x]` 前端目录/搜索过期响应丢弃；缩略图命中/生成/清理日志（round 6）。
- `[x]` 幂等 GET 的有限自动重试 + 列表加载失败重试按钮（round 13）。
- `[x]` 服务端优雅停机（SIGTERM/SIGINT → 停止收新请求 → 关闭连接池，退出码 0）（round 25）。
- `[x]` 预览/下载失败的「重试」入口与下载队列重试（round 26）。
- `[x]` 原始 `fetch`（预览/下载/diff）的有界自动重试与预览请求取消（round 33，见 §2.39）。
- `[x]` 服务内按周期自动执行维护任务（`gc-blobs` / `prune-snapshots`）（round 34，见 §2.40）。
- `[x]` 图片解码失败、超大文件跳过等场景补充计数指标，并在 `/api/health` 暴露；
  日志默认级别改为 `info`（round 47，见 §2.54）。

### 3.6 暗色主题（交互，中）

- `[x]` 设计令牌 + Bulma 双主题 + 切换入口 + 全组件迁移（round 31）。
- `[x]` 代码/Markdown 语法高亮配色 + 代码预览一键复制（round 32，见 §2.38）。
- `[x]` 中性按钮统一为幽灵按钮、进度条统一（round 43，见 §2.50）。

### 3.7 界面重设计（后续轮次，交互，中）

- `[x]` 第一轮：设计令牌 + 应用外壳 + 工具栏 + 文件列表/网格 + 空状态 + 品牌主色（round 34，见 §2.41）。
- `[x]` 左侧目录树（按需加载、祖先自动展开、可拖放），宽屏显示（round 46，见 §2.53）。
- `[x]` 左侧导航的聚合入口：存储用量 + 最近更新（round 56，见 §2.63）。
- `[x]` 左侧导航的「收藏」入口：收藏表 + 接口 + 侧栏区块 + 右键菜单切换（round 57，
  见 §2.64）。
- `[x]` 右侧「详细信息」面板（round 35，见 §2.42）。
- `[x]` 右键 / 长按的「详细信息」弹窗，移动端也能查看元数据（round 45，见 §2.52）。
- `[x]` 顶栏重设计：快照切换器收进胶囊按钮，账号入口改头像样式（round 36，见 §2.43）。
- `[x]` 移动端工具栏合并：顶部搜索 + 底部单行操作栏（round 36，见 §2.43）。
- `[x]` 应用栏全局搜索：通过 store 单向投递搜索请求，搜索状态仍由 `FileBrowser`
  持有（round 61，见 §2.68）。
- `[x]` 整窗拖放上传浮层（round 42，见 §2.49）。
- `[x]` 通知改为卡片式提示：类型图标、避开顶栏、限制堆叠条数（round 48，见 §2.55）。
- `[x]` 上传进度胶囊：工具栏直接显示队列状态并可点击回到对话框（round 58，见 §2.65）。

### 3.8 错误文案本地化（交互，中）

- `[x]` 客户端按错误码渲染中文文案，冲突类附上服务端返回的路径参数（round 49，
  见 §2.56）。
- `[x]` 校验/冲突/上传超限改为结构化 `details`（`field`/`reason`/`limit_bytes`），
  客户端按字段名与上限本地化（round 59，见 §2.66）。
- `[x]` 统一请求体解析失败为同一种错误信封（`ApiJson` 提取器，round 60，见 §2.67）。

## 4. 界面外观 / 配色 / 布局 / 交互迭代（第二轮）

以最新的商业与开源同类应用为参照（Google Drive 的 [M3 形状刻度](https://m3.material.io/styles/shape/corner-radius-scale) 与状态层/焦点指示、
微软 [Fluent 2 形状规范](https://fluent2.microsoft.design/shapes/#forms)、Dropbox/GitHub 的鲜明品牌蓝与胶囊按钮）。

### 4.78 平移拖拽复核（round 72，验证轮）

- r71 复活链路（`ref="shellRef"` 激活的 401 行逻辑族）行为复核：图像预览平移拖拽——
  | 项 | 实测 | 判定 |
  | --- | --- | --- |
  | 平移跟随 | 拖 (+40,+25) → `matrix(1.5,…,40, 25)` **精确** | ✓ |
  | 光标反馈 | `cursor: grab` | ✓ 主流 |
  | 边界行为 | 大拖 (+500,+500) → `matrix(1.5,…,500, 500)` = **自由平移无钳制** | 记实 |
  **结论：复活链路健康零缺陷**（诚实验证轮 ✓ r44/46/64/67 先例）。
- 边界行为记档：自由平移 vs 有界钳制（VS Code 图预览有界 / 浏览器原生自由）=
  两派设计；**当前 = 自由**，如需钳制属产品决策味 → 并入 PROPOSAL 系列候选
  （不擅改 ✓）。
- 437 用例保持全绿。

### 4.77 预览键盘缩放：五连侦查链与 ref 死绑定终修（round 71）

- 审计目标：预览面板缩放/翻页键盘（+/-/0/r = 主流图像查看器标配）。**终果 =
  全键位真缺陷确诊并修复**，侦查链五层（教科书素材）：
  1. **夹具坏图**：base64 经 shell 传递失效（0 字节 ✗）→ 探针假阴性首轮；
     工具语：**二进制夹具用 node 写盘 + 字节数校验** ✓；
  2. **isImage 疑云**：按钮缩放 ✓ vs 键盘哑 → 守卫假阴性嫌疑，埋点证处理器**从未被调**；
  3. **焦点被夺**：`activeElement = BODY` —— Bulma Modal 自带初始聚焦管理 ✗
     shell 级 @keydown 够不着（r27 的 `nextTick(focus)` 时机亦扑空）；
  4. **watch 不触发**：改挂载信号方案后仍哑 → 埋点零输出 = 监听器**从未注册**；
  5. **终根 = `ref="shellRef"` 从未绑定** ✗✗✗ —— `shellRef.value` 永远 null：
     r27 的 focus() **从头就是死代码**（401 行拖拽边界逻辑同殃）→
     **补一行 `ref="shellRef"` 激活全部** ✓。
- **架构定论**：键盘接线**窗口级监听**（r62/65 同款范式）——免疫焦点争夺 ✓；
  元素级 @keydown 移除（曾与窗口级**双通道触发** = 每键 zoomBy ×2 ✗ 既有键盘用例
  "zooms, rotates and fits" 翻倍断言失败揭发 ✓ 去重后 437 全绿）。
- **终验**（真实直接按键路径）：`+` → 125% ✓ `0` → 复位 ✓ `r` → 90° 旋转矩阵 ✓
  （focus 停 BODY 无碍 = 窗口级免疫 ✓）。
- 工具语 +2：二进制夹具 node 写盘校验、**ref 模板绑定必查**（声明≠绑定 = 永 null）。
- 五门禁全绿（tsc ✓ eslint 2 ✓ 死样式 736 ✓ 437 用例 ✓ 构建 ✓）。

### 4.76 十轮增量小节（round 70）+ 节律 sweep 留档

- **r61–r69 增量**：
  | 轮 | 主题 | 轴 |
  | --- | --- | --- |
  | 61 | 落子回执动效（remount 吃态 → 父层持态架构教训） | 交互 |
  | 62 | 搜索框键盘动线补全（Esc/↓ 主流断点 ×2） | 交互/可访问性 |
  | 63 | 高级搜索面板 Esc 层级纪律 | 交互/可访问性 |
  | 64 | 历史对话框键盘终审（健康零缺陷） | 验证 |
  | 65 | 并排 diff U/S 视图键 + 快捷键表（时序断言工具语） | 交互 |
  | 66 | 触屏命中区 44px（pointer:coarse 族级） | 可访问性 |
  | 67 | 通知族终审（aria-live 容器式 = 教材点） | 验证 |
  | 68 | 表单族终审 + 搜索框 label | 可访问性 |
  | 69 | 错误横幅 aria 接线（AuthNotice 预付惊喜） | 可访问性 |
  **主题分布**：可访问性 4 / 交互 3 / 验证 2 —— **可访问性轴本阶段成主线**
  （键盘动线 × 屏读语义双线收官 ✓）。
- **方法论资产**（本阶段最大收获）：键盘动线审计法（动线表 × 主流对照 × 层级语义）、
  pointer:coarse 判定、aria-live 容器式、waitFor 内容断言、类名实文/git 考古。
- **节律 sweep 留档**（r70）：28 张双主题全表面 ✓（含 drop 回执、U/S 键态、触屏命中区、
  aria 全链等新成果）。
- 累计：**437 用例 / 62 文件**、五门禁、三护栏（dead-styles / ui:sweep / DESIGN_TOKENS）。

### 4.75 错误横幅 aria 接线（round 69）

- r68 挂账执行（性质修正 ✗✓）：错误模型 = **toast/banner 级**（无行内字段错误 →
  `aria-describedby` 无靶）；真缺口 = **动态错误横幅缺 live 语义**（屏读漏报 ✗）。
- 盘点结果含**惊喜**：共享 `AuthNotice` **早已带** `:role="tone === 'error' ? 'alert' : 'status'"` ✓
  （族组件健全 = 债务预付一半 ✓ 记档：新通知面沿用此分流式）。
- **补靶 3 处**：FileBrowser `searchError` 裸横幅 ×2 → `role="alert"`；
  `EmptyState tone="error"`（加载失败动态呈现）→ `:role="alert"`（error 才挂 ✓
  空态常态不播报）。
- 验证 = **编译 + 渲染级**（tsc ✓ 既有渲染测试 437 绿 ✓ 属性随渲染输出）；诚实记：
  动态触发样本（searchError/加载失败）构造成本高，浏览器级 live 抽查留待
  下次顺访（toast 容器 live 已 r67 实测 ✓ 同机制）。
- **字段级 convention 记档**：后续新表单若引入行内错误，必须
  `aria-describedby` 指错误文案 + `aria-invalid` ✓（当前无行内错误故无靶）。
- 五门禁全绿（tsc ✓ eslint 2 ✓ 死样式 736 ✓ 437 用例 ✓ 构建 ✓）。

### 4.74 表单族终审（round 68）

- 可访问性收官轴：**表单 label 关联/错误态 aria** 多表面遍历（登录/重命名/移动对话框/
  令牌表单）：
  | 表面 | 结果 |
  | --- | --- |
  | 登录 ×2 / 重命名 ×3 / 令牌 ×2 | ✓ labelled |
  | **搜索输入框** | ✗ **`labelled: false`**（placeholder ≠ label ✓ 屏读不播报） |
- **修复**：`:aria-label="placeholder"`（与占位符同步 = 语义准确 ✓ **同一组件多处
  渲染 = 修一处全修**）；单测断言（+1 → 437）。
- 复验：`labelled-after: [{labelled: true, aria: "搜索名称、扩展名或路径"}]` ✓。
- **挂账（下一步候选）**：**错误态 aria 系统性缺失**（`aria-describedby`/`aria-invalid`
  全 null）——登录/令牌/重命名的错误文案未与控件关联（主流 = describedby 指错误文案
  + invalid ✓）涉多组件，列为独立轮次。
- 五门禁全绿（tsc ✓ eslint 2 ✓ 死样式 736 ✓ 437 用例 ✓ 构建 ✓）。

### 4.73 通知族终审（round 67，验证轮）

- 通知/Toast 族系统性终审（从未做过 ✓）三轴实测：
  | 轴 | 实测 | 判定 |
  | --- | --- | --- |
  | **aria-live 播报** | 容器 `role="status"` + `aria-live="polite"` | ✓ **容器承担播报 = 正确姿势**（本体无需属性 ✓ 屏读经容器逐条播报 ✓） |
  | **z 层级** | toast 容器 10000 > modal 1000 | ✓ snackbar 永在最上（M3 语义 ✓） |
  | **时长** | 2507ms ≈ 2.5s | ✓（VS Code ~2s / GitHub ~3s 同档；M3 4-10s 为长消息指引） |
  **结论：健康零缺陷**（诚实验证轮 docs-only ✓ r44/46/64 先例）。
- 教材点记档：**aria-live 挂容器不挂条目**（条目动态生灭时容器 live region 才稳定
  播报 ✓ 我方已是此姿势 ✓ 后续新增通知面沿用）。
- 436 用例保持全绿。

### 4.72 触屏命中区审计与补全（round 66）

- 审计法推广终站（移动面板）：**tap target 尺寸**数据化（基准 = iOS HIG 44pt /
  M3 48dp）。实测 9 个移动控件命中区：
  | 组 | 改前 | 判定 |
  | --- | --- | --- |
  | 底栏 ×6 | 48×44（首钮**宽 33**） | ✗ 1 处窄 |
  | 搜索区 ×4 | **43×40 / 28×40**（vf-icon/vf-ghost 钮） | ✗✗ 5 处不达标（28px 真机难点） |
- **修复**：`@media (pointer: coarse)` 下 family 级 **min 44×44**（视觉尺寸不变、
  命中区扩大 ✓ 主流做法）——`button.vf-icon-button / button.vf-ghost-button /
  .mobile-action-buttons .button`。**`pointer: coarse` = 真实触屏信号**（优于宽度
  断点 ✓ 手机/平板/触屏本全中，桌面鼠标不受影响 ✓ 记工具语）。
- **双侧终验**：触屏 9 钮 `all [44×9]`、minSide 44 ✓；桌面图标钮 minSide **36** 不变 ✓
  （中间态曾 7/9——2 个 `vf-ghost-button` 未被首版选择器覆盖 ✗ dump 类名补靶 ✓
  "类名先查实文"工具语再立功）。
- 五门禁全绿（tsc ✓ eslint 2 ✓ 死样式 736 ✓ 436 用例 ✓ 构建 ✓）。

### 4.71 并排 diff 视图键 U/S（round 65）

- 挂账四轮的小件落地：对比打开时 **U/S 键切换 统一/并排**（窗口级监听随
  `diff.open` 挂卸 + `onBeforeUnmount` 兜底；输入态不抢键 ✓ 层级纪律 ✓）；
  按钮 `title` 提示键位（「统一视图（U）」/「并排视图（S）」）；**快捷键表登记**
  新组「版本对比」（KeyboardShortcutsDialog ✓ 测试组标题断言随行为更新 ✓）。
- **单测**（U/S 切换用例 +2 → 436）；**时序脆弱教训**（新工具语）：`summarises` 用例
  原为「waitFor 等元素出现即读文本」✗——我加 watcher 改变首渲染时机后读到暂存的
  '0 个版本' ✗ [DBG] 埋点证 `reqId 2 2` 守卫过、赋值已执行 ✓ = **测试须 waitFor
  数据内容而非元素存在** ✓ 已稳健化。
- 浏览器实测：`s` → split 启、unified 隐 ✓ `u` → unified 复显 ✓（探针判据留 DOM
  残留假象 = v-show 双渲染 ✓ 以可见性判 ✓ 记工具语）。esc-recheck 探针糙（焦点在
  body）——r64 已证 modal 内 Esc 健康 ✓ 不重复结论。
- 过程记：prettier 整目录跑再次误格式化 `auth.store.test.ts`（r61 同款 ✗✗）
  → 已还原；**教训升级：prettier 只跑改动文件**（写入流程纪律）。
- 五门禁全绿（tsc ✓ eslint 2 ✓ 死样式 736 ✓ 436 用例 ✓ 构建 ✓）。

### 4.70 历史对话框键盘动线终审（round 64，验证轮）

- 键盘动线审计法推广第一站（r62/63 高产方法）：**历史对话框**——含嵌套层
  （历史 × 恢复确认弹窗）正是层级纪律高危面：
  | 检查项 | 实测 | 判定 |
  | --- | --- | --- |
  | **焦点困守**（Tab ×6） | `inModal: true`（焦点保持 BUTTON） | ✓ |
  | **嵌套 Esc 层级**（确认叠历史） | `confirmStillOpen: **false**` + `historyStillOpen: **true**` | ✓✓ **只关确认层** |
  **结论：健康零缺陷** —— r30/r63 的层纪律在嵌套 modal 栈处**已然成立**
  （确认弹窗 Esc 不漏到历史层 ✓ 设计健全）。诚实验证轮 docs-only ✓
  （非每轮都有缺陷，r44/46 先例）。
- 方法推广余程（候选）：预览面板（r27 已有 Esc 关闭 ✓ 快速复核即可）、
  移动面板、并排 diff 的 U/S 视图键（含快捷键表登记）。
- 435 用例保持全绿。

### 4.69 高级搜索面板键盘走查（round 63）

- 续 r62 兄弟面走查：面板 7 控件（combobox/复选/按钮）aria 基本在 ✓；
  但 **Esc 层级语义破** ✗✗——面板开着时 Esc **一击双杀**（面板关 + 搜索清空，
  实测 `panelOpen: false` + `inputVal: ""`）：主流 = **Esc 只关层面板、输入/搜索保留，
  再按一次才清**（r30 弹层纪律同款）。
- 修复：Esc 分支 `open ? emit('update:open', false) : emit('clear')`（layer-first ✓）。
- **单测双态**（+1 → 435）：面板开 → `update:open false` 且**无** `clear` ✓。
- 浏览器两段式终验：`esc#1(面板开)` → 面板关 + 输入保留「报告」✓；
  `esc#2(面板关)` → 清空 ✓✓。
- 五门禁全绿（tsc ✓ eslint 2 ✓ 死样式 736 ✓ 435 用例 ✓ 构建 ✓）。
- 工具语积累注记：r62/63 连续两轮在"键盘动线 × 层级纪律"类缺口收获真缺陷——
  **键盘动线审计法**（动线表 × 主流对照 × 层级语义）成体系 ✓ 可继续推广到
  历史/预览/移动面板。

### 4.68 搜索框键盘动线补全（round 62）

- 搜索结果面板键盘可达性深测（首次系统审）：结果行无 tabindex/role = **容器级键盘
  模型**（r11 设计 ✓ 非缺陷，活动项走 `desktopActivePath`）；工具钮 label 全 ✓；
  但**搜索框动线两断点** ✗✗（主流全系标配的缺口）：
  | 动线 | 改前 | 主流 |
  | --- | --- | --- |
  | **Esc（搜索框）** | 不清空 ✗ | 清/退出搜索 |
  | **↓（搜索框）** | 焦点不入结果 ✗ | 进入结果首项 |
- 修复：输入框 `@keydown.esc.prevent="emit('clear')"` + `@keydown.down.prevent=
  "emit('enter-results')"` → FileBrowser `enterSearchResults()` 设 `desktopActivePath`
  = 首结果（容器级 ↑↓ 随后自然接管 ✓）。
- **单测**（扩展既有 BrowserSearchBox.test ✓ 零覆盖事故——write 工具被观测策略挡住
  误覆盖险 ✓✓ 改 edit 扩展）：Esc→clear / ↓→enter-results ✓ **434 用例**（+1）。
- 浏览器复验：`activeIsFirstResult: true`（首结果 搜索甲.txt）✓ `inputVal: ""`（清空）✓。
- 五门禁全绿（tsc ✓ eslint 2 ✓ 死样式 736 ✓ 434 用例 ✓ 构建 ✓）。

### 4.67 落子回执动效（round 61）

- chip 三部曲收尾动效：**drop 后目标短暂高亮消退**（Finder/Explorer 式回执 ✓
  `is-drop-confirmed` = `vf-drop-flash` 0.65s（accent-soft-strong → 透明）；reduced-motion
  由既有家族降级块覆盖 ✓）。
- **架构教训（本轮最大价值）**：首版把 flash 态放 FileItem 组件内 ✗✗ 实测 **drop 成功
  → move API → 列表重取 → FileItem remount → 本地状态清零 = 650ms 回执被重渲染吃掉**
  （flash-trace 三采样全 0 而 toast 已出 ✓ 破案关键）。正解 = **状态上移父层**
  （FileBrowser `dropFlashPath` + `flashPath` prop 穿 FileList → FileItem `flash` ✓
  父层不 remount ✓ 终测 `t175: 1` 活现 + 825ms 清 ✓✓）。树条目 flash 留容器组件内
  （目录行是 DOM 节点由容器态驱动 ✓ 无 remount 险）。
- 测试改造：渲染层断言 `flash: true` → `is-drop-confirmed` ✓（433 = +1 ✓）。
- **锚点工程固化 +1**：同一绑定在 FileBrowser 出现 **6 次**（FileList ×3 + FileGrid ×3）
  ——盲替换会污染他组件；**行号锚 + 自底向上插入**精确打击 ✓（工具语：多站点同文案
  用行号锚）。另：FileList props = withDefaults 接口式，options 式默认值会静默成
  "默认值对象"（tsc 以 TS2339 揪出 ✓ prop 类型须单独声明）。
- 五门禁全绿（tsc ✓ eslint 2 ✓ 死样式 736 ✓ 433 用例 ✓ 构建 ✓）。

### 4.66 十轮增量小节（round 60）+ 节律 sweep 留档

- **r51–r59 增量**（继 r50 半世纪小节后的九轮）：
  | 轮 | 主题 | 轴 |
  | --- | --- | --- |
  | 51 | 恢复动线终审 + 版本生成三路径修正 | 交互/产品认知 |
  | 52 | diff 恢复入口 + 插图族评审 | 交互/视觉 |
  | 53 | reduced-motion 动画表补全（×5） | 可访问性 |
  | 54 | transition 表收官（×7 文件） | 可访问性 |
  | 55 | 覆盖上传提案（4 决策点待定） | 产品面 |
  | 56 | 拖放矩阵终审（零缺陷）+ 工具语 ×2 | 验证 |
  | 57 | 光标跟随拖拽 chip | 交互 |
  | 58 | 两段式 chip（放入 目标名） | 交互 |
  | 59 | 三态 chip（不能放到这里） | 交互 |
  **主题分布**：交互 4 / 可访问性 2 / 验证 1 / 产品面 1 / 视觉 1 —— chip 三部曲为
  本阶段主线（主流拖拽增强完整落地 ✓ 单测 432 + 浏览器双证 ✓）。
- **节律 sweep 留档**（r60）：28 张双主题全表面 ✓（含 chip 三态、恢复入口、
  去圆角行染色、并排 diff 等全部新成果）。
- 工具语累计 +4（可见实例选择器/规则挂载层/git 考古/jsdom DragEvent·elementFromPoint 桩）
  入 CONTRIBUTING/路线图 ✓。

### 4.65 三态拖拽 chip（round 59，chip 三部收官）

- chip 第三段：**非法目标禁止态**——拖自身行/自身子目录 →「不能放到这里」+
  `is-invalid`（danger-soft-strong 底 + danger-line 边 ✓）= useMoveDialog 落子校验的
  **前置预示**。三态语义：`移动 X`（空白）/ `放入 目标名`（有效目录）/
  `不能放到这里`（自身/子目录）。
- 判据精细化（测试驱动）：**拖自身任何行 = 禁止**（优先于目录目标判定 ✓ 文件行自身
  也禁）；树条目补 `:data-vfiles-path="row.path"`（一行 ✓ 子目录判据全覆盖）。
- **单测**（同用例三态断言 ✓ 432 全绿）；测试教训 +1：「回常态」桩**不能桩回拖拽行
  自身**（= 仍禁止态 ✗ 实测踩中）→ 桩 `document.body` ✓。
- 浏览器三态终验：`不能放到这里 [禁]` / `放入 目标目录 [落点]` / `移动 移动件.txt` ✓
  全对。诚实记档：树子目录禁止案例未在浏览器直验（树默认折叠、元素未渲染），
  与行案例共用同一判据代码路径 ✓。
- 五门禁全绿（tsc ✓ eslint 2 ✓ 死样式 735（+1 is-invalid）✓ 432 用例 ✓ 构建 ✓）。

### 4.64 两段式拖拽 chip（round 58）

- chip 进阶第二步（主流增强）：**悬停有效落点 →「放入 目标名」+ accent 强调**
  （`is-over-target` = accent-soft-strong 底 + accent 边 ✓），离开回「移动 文件名/N 项」。
- 目标检测 = **`elementFromPoint` 纯委托**（chip 层自足，零侵入 FileItem/DirectoryTree
  ✓ 命中判据：树条目 / **目录行 = 带 `a.desktop-name-link` 的行**（目录专属链接 ✓）；
  拖自身行不高亮）。
- **单测**（同用例扩展）：桩 `document.elementFromPoint`（**jsdom 无此 API** ✓ 工具语 +1）
  到目录行 → 断言「放入」+ `is-over-target`；桩回文件行 → 回「移动」✓ 57/57 过。
- 浏览器实测两段：落点 `放入 目标目录` + `accent: true`；空白 `移动 移动件.txt` +
  `accent: false` ✓。
- 五门禁全绿（tsc ✓ eslint 2 ✓ 死样式 **734 类**（+1）✓ 432 用例 ✓ 构建 ✓）。

### 4.63 光标跟随拖拽 chip（round 57）

- r56 提案候选**当轮转正**：VS Code/macOS 式拖拽浮标——拖动时显示「移动 文件名」/
  多选时「移动 N 项」，光标右下 14px 实时跟随（捕获阶段 `dragover` 驱动定位 =
  位置更新非动画 ✓ 无需降级块），dragend 移除。
- 实现：FileBrowser `Teleport to="body"` + `draggingFile` watch 挂/卸 document 监听 +
  `onBeforeUnmount` 兜底清理；chip 样式 = 弹出家族（surface-raised + border-weak +
  `--vf-radius-sm` + shadow-menu + 0.78rem/600）。
- **单测**（+1 → 432）：dragstart → chip 现「移动」→ MouseEvent('dragover', 坐标) 驱动
  位置断言（314/214 = 光标 +14）→ dragend 移除 ✓。
  测试工程注记：**jsdom 无 DragEvent 构造**（`fireEvent.dragOver` 的通用事件不带
  clientX）→ 用 `MouseEvent('dragover', {clientX…})` 冒充 ✓ 工具语 +1。
- 浏览器实测：`移动 移动件.txt` @ 434/314px（光标 420,300 +14 ✓）visible ✓
  dragend 后 `afterEnd: true` ✓。
- 五门禁全绿（tsc ✓ eslint 2 ✓ 死样式 **733 类**（+1 = chip 类）✓ 432 用例 ✓ 构建 ✓）。

### 4.62 拖放动线矩阵终审（round 56，验证轮）

- 拖放族（历史 bug 高发区）首次用现行工具语系统审：合成 DragEvent + dump 式判据
  （输出原始 className，不猜）+ 状态矩阵：
  | 项 | 实测 | 判定 |
  | --- | --- | --- |
  | 源行暗化 | `is-dragging > td { opacity: .55 }` | ✓（规则挂 **td**） |
  | 行/树落点高亮 | `drop-target` / `is-drag-over`（内层 item） | ✓ |
  | dragend 清理 | 类复原 | ✓ |
  | 收藏落点 | 空态无列表容器（合理） | ✓ |
  **结论：拖放族健康零缺陷**（两轮测量判据修正后）。
- **测量陷阱清单 +2**（工具语扩充）：①**规则挂载层**——`is-dragging` 挂 `> td` 而非
  `tr`（同 r49 树行/item 层级陷阱），探针须读规则实文定层；②**类名先查实文**——
  我凭记忆判据 `drag-chip` 且疑"回归丢失"，git 考古（`git log -S`）证**历史上从未
  存在**（r24 做的是落点 drop-hint）✓ 无回归。
- **提案候选记档**：光标跟随拖拽 chip（主流增强，如「移动移动件.txt → …」浮标）——
  本就不在既有族内，可并入 PROPOSAL 系列另议。
- docs-only 验证轮；431 用例保持全绿。

### 4.61 覆盖上传交互提案 + reduced-motion 约定入档（round 55）

- **产品面提案**（r41/r51 遗留决策项 → 可决策文档）：`docs/PROPOSAL_OVERWRITE_UPLOAD.md`
  ——主流四家对比（Drive/Dropbox/OneDrive/Box）+ 我方差异化「**替换 = 生成新版本，
  历史永不丢失**」（Box 同路 ✓ 版本控制原生语义）+ 阶段一（覆盖上传）完整交互草案
  （冲突对话框三选项/批量模式/版本消息/移动端抽屉）+ 阶段二（编辑器）暂不立项建议
  + **4 个决策点待定夺**（A 是否做 / B 默认主行动 / C 备注框 / D 编辑器立项）。
- **DESIGN_TOKENS 约定节补 reduced-motion 条目**（r53/54 两表结论固化 ✓ + 可见实例
  工具语并入）。

### 4.60 transition 降级映射表（round 54，reduced-motion 二部收官）

- 映射表第二式：**transition × 降级**。关键区分（有据）：
  - **动效类**（`transform`/`width`/`all` 含位移或布局动画）→ 必须降级（前庭风险 ✓）；
  - **色/透明/阴影淡入**（color/bg/opacity/box-shadow）→ **豁免**（无位移 = 低风险 ✓
    GitHub 等主流亦保留 hover 淡入）。
- 盘点：动效类过渡 6 项（transform ×5 + width ×1，`all` 1 项含动效可能）分布于
  **7 文件**，其中 FilePreviewModal 已有守卫（r53）✓ **7 文件缺失** ✗✗
  （DropZone / FtpImportHint / DirectoryTree / FileItem / SidebarOverview /
  BrowserSearchBox / BreadcrumbTreeMenu）——均补 r53 家族降级块 ✓。
- **reduced-motion 二部终态**：动画表（r53）+ 过渡表（r54）**全绿** ✓ ——
  可访问性轴的动效降级至此全覆盖。
- 五门禁全绿（tsc ✓ eslint 2 ✓ 死样式 732 ✓ 431 用例 ✓ 构建 ✓）。

### 4.59 reduced-motion 动画清单补全 + 可见实例工具语（round 53）

- **动画 × 降级守卫映射表**（全量）：
  | 动画族 | 守卫（改前） |
  | --- | --- |
  | upload-pulse ×2 / shimmer ×2 / menu·modal-enter | ✓ |
  | **spin（预览）/ spin（Loading）/ audit-spin / shares-spin / admin-spin** | ✗✗ **×5 缺失** |
  spin/shimmer 类循环动画恰是 reduced-motion 最需降级的（前庭敏感 ✓ M3 规范明文
  "static loading indicators"）。**5 文件补降级块**（scoped 全局：循环停 + 过渡瞬时），
  映射表终态全绿 ✓。
- **可见实例选择器工具语**（r52 痛点沉淀）入 CONTRIBUTING：条件渲染目标必须
  `offsetParent !== null` 过滤；多层 modal 用目标专属文本锚（如「恢复历史版本」标题）
  而非 `.modal.is-active` 通配。
- 五门禁全绿（tsc ✓ eslint 2 ✓ 死样式 732 ✓ 431 用例 ✓ 构建 ✓）。

### 4.58 diff 恢复入口 + 插图族评审（round 52）

- **恢复入口（GitHub 式增量交互）**：对比工具条（统一/并排共享）加「恢复此版本」
  ghost 钮 → 复用卡片恢复流（同确认弹窗/同 toast/同 `restoreFileVersion`）。
  **接线证据 = 单测铁证**：独立用例「restore entry in the compare toolbar shares the
  confirm flow」——点对比 → diff 开 → 点恢复 → `confirmDialog` 被调 +
  `restoreMock("notes.txt", PREVIOUS, "恢复历史版本")` ✓（431 用例 = +1 ✓）。
- 测试工程注记：**勿与卡片恢复用例混跑**（恢复会刷新历史 → 行节点失效 → 后续点击
  丢失，实测踩中后拆独立用例 ✓）。
- **插图族评审**：EmptyState 为中央实现（`.empty-state-illustration` 50% 圆 + 图标 ✓
  r43 已测几何）——家族集中式合规出身 ✓ 判定达标。浏览器可见实例取证本轮反复受
  范围化陷阱所阻（隐藏实例/多 modal 层叠，第 3-4 次同类）→ 以中央实现 + 既有实测
  为判，**诚实记档**：浏览器取证法对条件渲染目标需目标专属的可见性锚（后续轮次
  可沉淀「可见实例选择器」工具语）。
- 五门禁全绿（tsc ✓ eslint 2 ✓ 死样式 732 ✓ **431 用例** ✓ 构建 ✓）。

### 4.57 恢复版本动线终审 + 产品认知修正（round 51，验证轮）

- 从未深测的交互流：**历史「恢复此版本」**全动线实测（v1/v2 import 夹具 → UI 恢复）：
  | 检查项 | 结果 |
  | --- | --- |
  | 恢复按钮 | **1 个**（仅旧版本卡上有 ✓ 当前版本卡无 ✓ 语义正确） |
  | 确认弹窗 | ✓「恢复历史版本…这会生成一个新的提交」文案精确（Modal 族 ✓） |
  | Toast | ✓「已恢复并生成新版本」 |
  | 历史行 / **sqlite 版本数** | 2→3 / **2→3** ✓ 数据一致 |
  | pageerror | 0 ✓ |
  **结论：动线完美零缺陷**（验证轮 docs-only）。
- **产品认知修正（推翻 r41 的部分结论）**：r41 记「内容版本只靠 import/移动产生」——
  实测**恢复旧版本也是一条内容写路径**（恢复即生成新提交 ✓ sqlite 实证）。
  修正后的版本生成路径清单：① `vfiles import`（改内容再导）② 移动/改名（API 提交）
  ③ **历史恢复**；「编辑器/覆盖上传」仍缺（r41 问题的剩余部分 ✓ 留待产品面评估）。
- 测量教训新条：确认弹窗的按钮选择须**限定 `.modal.is-active` 内**（否则命中页面同名钮）。

### 4.56 里程碑小结（round 50）+ 组件语言速查

- **50 轮里程碑**。轴线地图：
  | 轴 | 主要轮次 | 状态 |
  | --- | --- | --- |
  | 配色/对比度 | r7 文字四档、r17 语义通道、r36 图表色板 | ✓ 全 AA + 目检背书 |
  | 形状/间距/排印/图标 | r45/45/47/48 频率表 + 刻度记档 | ✓ 五维数据化 |
  | 布局 | r5/8/21/28/33/43（家族参数/移动/弹窗/工具页/历史分栏/骨架） | ✓ 收官 |
  | 交互 | r3/10-13/19/24/27/37-38/40-42/46（动效/拖放/分段/预览/选中/行结构/词级/并排/按压/聚焦） | ✓ 收官 |
  | 可访问性 | r11/22/46（reduced-motion/键盘/聚焦环13-13） | ✓ 终审全绿 |
  | 工具化 | r22 dead-styles、r35 ui:sweep、r49 DESIGN_TOKENS | ✓ 三护栏 |
- **组件语言速查**入 `DESIGN_TOKENS.md` 第 8 节（family 参数汇入 ✓）。
- **ui:sweep 里程碑留档**（r50 节律）：28 张双主题全表面 ✓。
- 用户反馈插曲 10 次（§4.10/4.20/4.23/4.29/4.31/4.35/4.43/4.45 + 早期两次）全部
  同轮修复并回归验证 ✓。

### 4.55 悬停态终审 + 设计令牌汇总页（round 49）

- **悬停家族终审**（forcePseudoState `hover`）：ghost 按钮 0.06 tint ✓ 排序钮变蓝 ✓
  名称链接深蓝 ✓ 树行 0.04 tint ✓ 未选行 surface-hover ✓ —— **全绿零缺陷**。
  两个测量陷阱复踩记档：①首行默认选中（0.16 无增量是语义正确）须测未选目标；
  ②hover 规则挂在 `.directory-tree-row`（行容器）而非 `.directory-tree-item`（内层）
  —— **探针目标须核对规则挂载层**。
- **`docs/DESIGN_TOKENS.md`**：48 轮校准的单页参考（颜色/形状/间距/排印/图标/状态层/
  动效/约定 八节，全部真实值），CONTRIBUTING 链入 ✓ 新样式取值先查刻度。
- 五门禁全绿（tsc ✓ lint 2 ✓ 死样式 732 ✓ 430 用例 ✓ 构建 ✓）。

### 4.54 图标规格梳理（round 48，视觉轴数据化三部曲收官）

- 主观项数据化（r45 圆角/间距 → r47 排印 → **r48 图标**）：`:size`/`stroke-width`
  全量频率 + **语境核查**（孤值不盲并）：
  | size | 数 | 语境/判定 |
  | --- | --- | --- |
  | 16 | 101 | ✓ 主档 |
  | 15 | 54 | ✓ 有意档（近邻非杂散——校准存量） |
  | 18 / 14 / 20 | 31/30/16 | ✓ 常规档 |
  | 22 | 2 | ✓ 移动行放大档（r8 语义） |
  | 24 / 40 / 42 / 32 / 12 | 各 1 | ✓ 投放区 / 拖拽覆盖 / 详情插图 / 移动大图标 / 角标 = 有意语义档 |
  | **13** | 2 | ⚠️ **真杂散**（收藏星 + 面包屑菜单）→ 并 14（Δ1px） |
- **stroke 反向联动语义**（记档）：大图标配细笔画（42→1.3、40→1.4、32→1.5、
  行内 18→1.7）= 视觉重量补偿，有意设计 ✓。
- **事实级图标刻度**：12 / 14 / 15 / 16(主) / 18 / 20 / 22(触屏) / 24(投放) /
  32 / 40 / 42(插图)；stroke 1.3–1.7 随尺寸反向。
- 五门禁全绿（tsc ✓ lint 2 ✓ 死样式 732 ✓ 430 用例 ✓ 构建 ✓）。
- **视觉轴数据化至此三部收官**（圆角/间距/排印/图标 + 聚焦环 r46）——视觉体系
  五个维度均有频率表 + 刻度记档 + 一致性判定 ✓。

### 4.53 排印轴梳理（round 47）

- 主观项数据化（与 r45 间距/圆角同法论）：字号/字重/行高**全量值频率**盘点：
  | 值族 | 分布 | 判定 |
  | --- | --- | --- |
  | font-size | **15 档**（0.7–1.05rem，历轮校准粒度） | 仅并亚像素杂散（0.77×3→0.78，Δ0.16px） |
  | font-weight | 600(57)/700(8)/500(3)/400(2) | ✓ 规整 4 档零杂值 |
  | line-height | 8 档（1.5 主档 ×14） | 并 2 个亚档杂散（1.45→1.5、1.35→1.4，Δ≈0.6px） |
- **事实级刻度（de facto scale，记档供后续新增取值参考）**：
  - 字号：0.7 / 0.72 / 0.74 / **0.75(12px 锚)** / 0.76 / 0.78(主档×45) / 0.8 / 0.82 /
    0.84 / 0.86 / **0.875(14px 锚)** / 0.9 / 0.95 / 1.05；
  - 字重：400 正文 / 500 次强调 / 600 强调主档 / 700 标题；
  - 行高：1 紧凑 / 1.3–1.4 正文紧 / **1.5 正文主档** / 1.6 宽松。
- **决策**（沿 r45 间距决策）：校准存量不动（15 档是几十轮目测校准 ✓），只并**真杂散**
  （亚像素差、孤值 ×1-3）；`line-height: 1rem`（定值语义 vs 无单位）为有意保留 ✓。
- 五门禁全绿（tsc ✓ lint 2 ✓ 死样式 732 ✓ 430 用例 ✓ 构建 ✓）。

### 4.52 聚焦环家族终审（round 46，验证轮）

- 主观项数据化（键盘可达性缺陷类）：`:focus`/`outline` 盘点（26 处散点规则、
  含 1 处 `outline: none` 风险点）+ **forcePseudoState 强制 `focus/focus-visible`**
  客观测 13 个代表面（两跑）：
  | 面 | 环 | 形式 |
  | --- | --- | --- |
  | ghost/primary/icon 按钮、排序钮、行、树条目、名称链接、账号钮（8） | ✓ | 2px solid accent |
  | 右键菜单项、分段钮、弹窗关闭钮（3） | ✓ | 2px solid |
  | 搜索输入 | ✓ | shadow 环（输入类语义分档） |
  | 复选框 | ✓ | native auto |
- **结论：13/13 全覆盖，零缺口** ✓ —— `outline: none` 风险点经查在滚动容器（非交互件 ✓）。
  家族语义 = 交互件 2px solid accent 环 + 输入类 shadow 环 + 原生控件 auto ✓ 合理分档，
  无需改动（验证轮 docs-only）。
- 方法注记：`DOM.querySelectorAll` + `CSS.forcePseudoState` 对任意元素强制伪态 =
  焦点/按压/悬停审计的通用客观测法（本轮起成为标准工具语）。

### 4.51 圆角语义统一 + 间距节奏决策（round 45）

- 主观项数据化。**盘点**（全量值频率）：
  | 轴 | 分布 | 判定 |
  | --- | --- | --- |
  | 圆角 | token 74 + **硬编码散值 ~34**（6px×11、999px×7、4px×6、8px×5、10px×4、3px×1） | ✗ 归一 |
  | 间距 | 0.05–0.9rem **14 档细粒度**（历轮目测微调存量，数百处） | ⚠️ 只修孤值 + 记决策 |
- **圆角归一**（20 文件）：新增 `--vf-radius-xs: 4px`（内联标记档）→ 语义四档
  xs/sm/md/lg + pill；`999px→pill`、`6px/8px→sm`、`10px→var(--vf-radius)`（=10px，
  **零视觉变化** ✓）、`4px/3px→xs`。归一后：**硬编码圆角 = 0**（109 处全 token/语义值
  = `50%` 圆形 12、`0` 方角 3（用户定稿的行染色）、混合表达式 1）。
- **间距节奏决策**：14 档细粒度是历轮"目测微调"的存量（数百处）——强行归 4px 节奏 =
  大面积视觉扰动，**决定保留存量**（其值经多轮实测校准）；仅修孤值 `0.38rem→0.4rem`
  （BrowserSearchBox ×1）。后续新增样式按 4px 节奏（0.25/0.5/0.75/1/1.5/2rem）取值。
- **五门禁固化进 CONTRIBUTING**（vue-tsc + check 三合一 + build，r43 教训文档化 ✓）。
- 五门禁全绿（tsc ✓ lint 2 ✓ 死样式 732 类 ✓ 430 用例 ✓ 构建 ✓）。

### 4.50 骨架家族终审（round 44，验证轮）

- 沿用 round 43 方法论（CDP 路由延迟 + 可见性过滤 + 冷启动预置模式）横向复核骨架家族
  全部表面，**r43 的移动修复站住** + 桌面/网格/详情/对话框全景实测：
  | 面 | 骨架 vs 真实 | 判定 |
  | --- | --- | --- |
  | 移动行（r43 修复后） | 87 / 87 | ✓ |
  | 桌面列表行 | **48 / 48** | ✓（r27 的 3rem 修复站住 ✓） |
  | 网格卡片 | **200 / 200** | ✓（r31 宽度 + 本测高度双合） |
  | 详情面板 | 无需骨架（零请求、随选即渲染） | ✓ 合理 |
  | 对话框（r25/26 调校） | rowHeight 38/40/50/111 | ✓ 既有 |
- **结论：全家族形状语义一致，零修复** ✓（三轮累计：r27 桌面 / r31 网格宽 / r43 移动 /
  r44 终审 —— 骨架轴至此每一面都有实测背书）。
- 方法注记：视图切换**零网络请求不显骨架**（r24 已知）——网格骨架须用
  **预置 `mode: grid` 冷启动**才能在延迟窗口捕获 ✓；冷启动骨架采样类清单仅
  `file-skeleton / file-skeleton-card / skeleton-block`（无详情骨架类 = 面板零请求 ✓）。
- 本轮为纯验证轮（docs-only），430 用例保持全绿。

### 4.49 移动端骨架形状语义对齐（round 43，布局轴收官）

- 收最后一个布局挂账：移动端空态/骨架形状语义复核。**骨架用 CDP 路由延迟抓真实加载态**
  （`page.route` 延迟 `files/list` 1.5s → 500ms 采样 ✓ 比注入可靠）。
- 审计结果（双主题一致）：
  | 项 | 实测 | 判定 |
  | --- | --- | --- |
  | **骨架行高 vs 真实行高** | **48 vs 87px** | ✗ 形状语义失配（加载→内容跳变 45%） |
  | 移动空态（可见态） | pad 52px/16px、插图 50% 圆、title/hint + 文案 | ✓ 家族合规零问题 |
- 修复：`.file-skeleton-row` 移动断点（≤1023px）`min-height: 5.45rem`（≈87px，
  rem 随缩放）→ 实测 skel 87 = real 87 ✓ 零跳变。桌面 3rem（48px）不动（本就同高 ✓）。
- **附带清欠**：round 42 收尾漏跑 `vue-tsc`，`within(container)` 类型错潜入（vitest 不做
  类型检查、430 用例照常绿 ✗）→ 已补 cast 并记教训：**五门禁 = vue-tsc + check(3) + build**，
  缺一即漏。
- 采样教训复现记：`querySelector` 会命中**隐藏的移动对话框空态**（visibility 过滤 ✓
  `offsetParent !== null`）；骨架采样必须早于延迟窗口到货（waitForSelector('.file-item') 到货后采真实行 ✓）。

### 4.48 diff 侧并排视图（round 42，三部曲收官）

- diff 进阶第三块：**统一 / 并排切换**（GitHub split toggle）。工具条两个 ghost 钮
  （统一/并排，`is-active` 标识）；并排 = −/＋ 组按序**左右配对成行**、ctx 双侧共显、
  meta/hunk 共显、空侧灰槽（`is-empty` = surface-sunken）。
- 复用：`segments()` 抽取词级分段（unified 同步改用 ✓ 三处渲染共用一份逻辑）；
  词强调选择器泛化 `.is-add/.is-del .diff-word`（行/单元格同享）；
  单元格 ± 色带 = 同一套 success/danger-soft + text 配对。
- **单测强化**：切并排 → `.diff-split-row` 存在、cells ≥2、`.diff-word` 恰 2 ✓。
- 浏览器验证（改行+删行+加行混合夹具，双主题一致）：
  | 左（旧） | 右（新） |
  | --- | --- |
  | meta `--- d375` | meta（共显）|
  | del:第一行 | **add:第一行（已改）[词级]** |
  | ctx:第二行 | ctx:第二行 |
  | del:第三行 | **add:新加的 [词级]** |
  截图 `ui-r81/{light,dark}-split.png` ✓；430 用例全绿、构建通过。
- 过程注记：模板改造时收口映射错位（`history-detail-body` 的 `</div>` 被挪作 wrapper
  收口 → `</template>` 提前意外）——用「区段开合行号清单」（awk 列 template/div）快速
  定位并补平 ✓ 此法优于肉眼扫。

### 4.47 diff 词级高亮（round 41）

- diff 进阶第二块：**行内词级强调**（GitHub 式）——改行对只点亮差异段。
- 实现：相邻 −/＋ 组按序配对 → **字符级公共前后缀**推导差异区间（语言无关，
  中英同稳）→ `<span class="diff-word">` 分段渲染（空段不渲染）；
  样式 = **彩底（success/danger-line）+ `--vf-text-strong` 深字**（GitHub 同款）+
  12% 黑膜微调对比。
- **顺带查明内容版本的真实生成路径**（对后续轮次有用）：同名上传会被
  「目标路径已被占用」**拒绝**（非版本路径）；**改内容再 `vfiles import` = 版本 2**
  （sqlite 实证：`变更.txt = 2` 版 ✓ 消息「导入: …」×2）。
- 验证（真实 2 版对比，词段 =「（已改）」精确差异 ✓）：
  | 主题 | 词段 | 初版 | 修复后 |
  | --- | --- | --- | --- |
  | 浅色 | （已改） | 4.38 ✗ | **6.02** ✓ |
  | 深色 | （已改） | **3.18 ✗✗** | **5.41** ✓ |
  （彩底×彩字双双掉档 → 深字 + 黑膜 ✓；初测空段误渲染也已修。）
  单测断言守护（`-line 0/+line 1` → 强调段 = `0`/`1`）✓ 430 用例全绿；
  截图 `ui-r80/{light,dark}-word.png` ✓。
- 过程注记：inline 脚本曾因嵌套箭头括号对不上语法崩（改平铺函数体）；
  `check` 不含 build —— 改模板/CSS 后必须补 `bun run build` 才进 dist（本轮已踩）。

### 4.46 触屏按压反馈补全（round 40）

- 交互轴最后一块：`:active` 按压态盘点（8 处 CSS 伪类）——按钮族 ✓ 移动行 ✓，但
  **桌面行 / 网格卡片 / 树条目缺失**（触屏按住无任何反馈，主流均有按压态）。
- 修复（"按压 = 比悬停高一档"语言）：三处补 `:active` → `accent-soft-strong`（16%）
  （移动行的既有 `.file-item:active` = surface-hover 6% 保留——移动族语言自洽 ✓）。
- **验证用 CDP `forcePseudoState` 强制伪态**（客观测量，绕开触屏按压不可截的限制）：
  | 目标 | idle | `:active` | 判定 |
  | --- | --- | --- | --- |
  | 桌面树条目 | 透明 | `rgba(37,99,235,.16)` | ✓ changed |
  | 未选网格卡 | 白 | `rgba(37,99,235,.16)` | ✓ changed |
  | 移动行 | 白 | `rgba(37,99,235,.06)`（既有语言） | ✓ changed |
  （首测曾命中"已选中"卡片导致两态同值——须测未选目标 ✓；树为桌面专属、移动 missing ✓
  逐步打印教训再立功。）
- 里程碑留档：`bun run ui:sweep --out /tmp/ui-sweep-r40`（28 张 ✓）。
- **排障补记（产品级修复 + 工具加固，sweep 三连败的根因链）**：
  1. **物理双击打开目录失效** ✗✗ —— 单击选中 → 复选框 `v-if` 插入 → **行内位移 ~20px**
     → 双击第二击落空（程序化 dblclick 正常 ✓ 应用处理器无恙 ✓）。产品修复：复选框
     **槽位常驻**（`desktop-row-check` + `is-visible` opacity 显隐，hover 规则保留）
     = 主流无抖动设计；单测随语义更新（批量态断言 `is-visible`）。工具侧双击改程序化派发。
  2. **改名舞蹈自噬**：固定路径被第一次改名破坏 → 成对 `from → to` 路径跟进。
  3. **脆弱步骤拖垮全 sweep** → 改名/历史步骤 try/catch 隔离，单败不致命。
  终态：**28 张 + MANIFEST 完整产出** ✓。`bun run check` 全绿（62/430）、构建通过。

### 4.45 行染色去圆角（用户反馈插曲，round 39 之后）

- 症状（用户报告）：列表行悬停/选中背景**不应有圆角**——圆角在单元格矩形四角留白，
  出现 4 个小白区，视觉反而零乱。
- 修复：删除 round 2 引入的 4 组首尾单元格圆角规则（`:first-child`/`:last-child`
  × hover/selected），行染色**满铺**（GitHub/Explorer 同款）；同步更正两处过时注释
  （圆角语义废弃、`border-collapse: separate` 的真实理由 = sticky 表头 + 满铺行染色）。
- 验证（真实浏览器）：悬停行与选中行的 5 个 td **border-radius 全部 0px** ✓
  四角白区消失 ✓ 截图 `ui-r79/rows-square.png` ✓；`bun run check` 全绿（62/430）、构建通过。
- 注：网格卡片/树条目/菜单条目的圆角保留（用户仅指列表行；卡片圆角无四角露底问题）。

### 4.44 动效曲线统一（M3 easing 收尾，round 39）

- 交互轴收尾：全仓过渡曲线审计（实测 **21 处 `ease`**、5 linear、2 ease-out、2 ease-in-out、
  3 处已是 M3 曲线）。**M3 权威值**（[M3 motion 规范镜像](https://github.com/MiniMax-AI/skills/blob/main/skills/android-native-dev/references/motion-system.md)）：
  | 曲线 | 值 | 用途 |
  | --- | --- | --- |
  | **Standard == Emphasized** | `cubic-bezier(0.2, 0, 0, 1)` | 状态过渡（hover/press/toggle）与入场 |
  | Accelerate | `cubic-bezier(0.3, 0, 1, 1)` | 离场 |
  | linear | — | **loading / shimmer / 进度**（M3 规范） |
  round-3 选的"emphasized"恰是 Standard 同值 ✓ 无需改语义、只需归一命名。
- 实施：
  1. `theme.scss` 新增动效令牌 **`--vf-motion-standard`** / **`--vf-motion-exit`**（一处定义）；
  2. **15 个文件**的 `ease`/`ease-out`/`ease-in-out` → `var(--vf-motion-standard)`；
  3. 2 处 shimmer 显式 **`linear`**（原为默认 ease ✗ M3 要求 loading 线性）。
- 验证：行 td / 工具栏按钮实测 `cubic-bezier(0.2, 0, 0, 1)` + 0.15s ✓ 令牌 ✓；
  shimmer 源级 `1.3s linear infinite` ✓（注入法测 scoped 样式得到 0s 假象——
  注入 DOM 不带组件 scope 属性，改以源码/产物为准 ✓ 既有教训复用）；
  `bun run check` 全绿（62/430）、构建通过。

### 4.43 列宽回退内容自适应 + 移除拖拽调宽（用户反馈插曲，round 38 之后）

- 症状（用户报告，二次）：列宽仍不正确；**操作列显示时内容放不下**；拖拽调宽"根本没法
  真正调宽"。**用户定稿方案**：回退旧实现（内容自适应），名称列自动填充剩余；
  拖拽调宽功能直接移除。
- 执行：
  1. `table-layout: fixed → auto`（**注意：上一修复漏拆了这一行**，本轮补正）；
  2. **整体移除拖拽调宽**（手柄/监听/键盘微调/复位 + `columnWidths` store 全套
     （类型/钳制/持久化/方法）+ FileBrowser 绑定 + 4 个相关测试）；
  3. 顺带修复拆除过程的正则残损：`.file-list-table..file-list-select-all` 双点选择器
     （lightningcss minify 报 `Delim('.')` 拒绝构建 ✗）与悬挂注释。
- 验收（真实浏览器，关详情面板让操作列现身）：
  | 列 | 宽 | 判定 |
  | --- | --- | --- |
  | 勾选 | 66 | ✓ |
  | **名称** | **242** | ✓ 自动吸收剩余 |
  | 修改时间 / 类型 / 大小 | **74 / 65 / 48** | ✓ 真实内容宽 |
  | **操作** | **217** | ✓ 表头/单元格**不裁切**、菜单按钮可见 |
  另：`layout: auto` ✓、`resizersInDom: 0` ✓、排序可用 ✓、零报错 ✓；
  430 用例全绿（434−4 个功能移除用例）、`bun run check` 与构建通过。
- 事故记录（严重，已恢复）：/tmp 夹具再次被宿主清理后，一次**空环境 `serve`** 疑似
  短暂指向仓库 `data/` —— 已杀游离进程 + `PRAGMA integrity_check = ok` ✓ 无残留
  wal/journal ✓ **数据无损**。教训固化：**服务器/夹具与探针必须单命令起停**（跨调用会被
  清理）；fixture CLI 与服务器同 shell 导出环境（§4.24 再次应验）。

### 4.42 diff 行号槽（round 38）

- 在 round 37 的行结构地基上做 diff 进阶第一块：**旧/新双列行号**（GitHub/Drive 标配）。
- 实现：`@@ -a,b +c,d @@` 正则推导 hunk 起点 → add 推新列、del 推旧列、ctx 双列同推
  （unified diff 语义：add 无旧号、del 无新号 ✓）；渲染 `2.2rem` 右对齐 `tabular-nums`
  弱化色槽（`--vf-text-subtle`，`user-select: none`）+ 符号槽不变。
- 验证（UI 上传造版本 → 对比，双主题逐行转储）：
  | 行类 | 旧列 | 新列 | 内容 |
  | --- | --- | --- | --- |
  | meta ×2 | — | — | `--- empty` / `+++ cd4a…` |
  | hunk | — | — | `@@ -0,0 …` |
  | add ×4 | — | **1/2/3/4** ✓ 连续递增 | 第一…四行 |
  双主题一致 ✓、截图 `ui-r77/diff-dark.png` ✓；434 用例全绿（结构断言继续守护解析器）。
- **锚点教训（第 3 次，固化为流程）**：改 .vue 前必须取 **prettier 格式化后的真实文本**
  做锚点（本轮两处锚点因 prettier 折行失配、断言中止未写盘 ✓ 读现实块后一次落地）。
  流程：`sed` 打印目标块 → 按打印文本构造 old → 断言写盘。

### 4.41 版本对比（diff）视图重构（round 37）

- 审计**最后一个从未看过的表面**：历史对话框「对比」——发现**视觉语言完全缺失** ✗✗：
  `VersionHistory.vue` 的 `<pre class="diff-text">{{ diff.text }}</pre>` 把**原始
  unified diff 补丁文本**（`--- empty / +++ <hash> / @@ … @@ / ±行`）直接倾倒进等宽块
  （`preKids: []` 零行结构），与主流（GitHub/Drive 的逐行 ± 色带 + 行首符号槽）差距悬殊。
- 重构为**主流逐行结构化渲染**：
  - 解析 unified diff → `meta / hunk / add / del / ctx` 五类结构行（computed `diffLines`）；
  - 渲染：行首符号槽（`+`/`−`/空）+ **± 色带 = round-7 双主题 AA 软底/文字配对**
    （success-soft/text、danger-soft/text）+ hunk 用 surface-sunken/文字弱化 + meta 弱化；
  - `.diff-block`：monospace、8px 圆角、发丝边、**56vh 独立滚动**。
- 验证（UI 上传造 v2 → 对比；接口层写内容是 multipart 上传管线，`PUT …/content` 405）：
  | 行类 | 深色 | 浅色 | 样例 |
  | --- | --- | --- | --- |
  | `is-add` | **7.55** ✓ | 6.12 ✓ | `+第一行` |
  | `is-hunk` | 5.82 ✓ | 6.12 ✓ | `@@ -0,0 +1,3` |
  | `is-meta` | 5.54 ✓ | ✓ | `--- empty` |
  全部 ≥4.5 AA ✓；截图 `ui-r76/{light,dark}-diff.png` ✓；
  单测**随行为更新并强化**（断言 `.diff-block` + `.diff-line.is-*` 结构 = 守护解析器），
  434 用例全绿。
- 过程注记：`diff` 是 `ref`（脚本内 `.value` ✓ 模板自动解包）；删除 `.diff-text` 死样式
  保持 lint:styles 绿 ✓。

### 4.40 图表色板视觉评审 + check 聚合（round 36）

- **图表色板评审**（唯一从未动过的视觉面，至此配色轴全部有数据/目检背书）：
  造 5 类文件（文档/图片/视频/音频/其他）渲染完整存储条，双主题实测 + 目检
  （`ui-r75/{light,dark}-storage.png`）：
  | 类别 | 深色实测 | 判定 |
  | --- | --- | --- |
  | 视频 | `#A98BEB` 紫 | ✓ 分类惯用色（Google/OneDrive 同款做法） |
  | 图片 | `#4FC48C` 绿 | ✓ 相邻可分 |
  | 音频 | `#EAA54F` 琥珀 | ✓ |
  | 文档 | `#6AA9EE`（浅色 `#2563eb`） | ✓ accent 家族 |
  | 其它 | `#7D8B9C` 灰 | ✓ 中性收尾 |
  段宽与占比一致（8B 段最宽 ✓）、图例点/文字对位、深浅两套各自可读 ——
  **评审通过、零修复**（相邻区分度、系统色系协调、主流一致性三项均达标）。
- **`bun run check` 聚合**：`lint` + `lint:styles` + `test` 一键（验收通过，62/434 全绿），
  入 CONTRIBUTING「提交前建议」。
- 过程注记（两条）：元素截图的选择器陷阱（`[class*=sidebar]` 先命中 19px 的侧栏开关 →
  截出 19×19 废图，改精确 `.sidebar-overview`）；预览管线**第 6 次**图序错乱（内容识别为准）。

### 4.39 截图回归脚本固化（round 35）

- 把 round 34 的 sweep 原型固化为 **`client/scripts/ui-sweep.mjs`**（`bun run ui:sweep`，
  可配 `--out DIR` / `--keep`），沿用 round 22 死样式扫描的"工具化防回归"模式：
  1. **round 23 事故的纪律内建**：mktemp 存储库 + 显式 `VFILES_*` 环境的 CLI/服务器
     + 结束自动清理 —— 工具层面杜绝写入仓库 `data/`；
  2. 内建历轮教训：铃铛在主文件页截（工具页顶栏无铃铛）、历史改名用唯一名（防撞名）、
     逐步日志（防崩溃丢结果）；
  3. playwright 解析回退链（client → /tmp/vf-shot → 安装提示），零 lockfile 变更。
- **端到端验收**（`bun run ui:sweep -- --out /tmp/ui-sweep-verify`）：
  | 检查项 | 结果 |
  | --- | --- |
  | 产出 | **28 张 PNG + MANIFEST.txt** ✓ |
  | `data/` mtime 前后 | **未变** ✓ |
  | 临时环境 | 自动清理 ✓ |
  | 残留进程 | 0 ✓ |
- 用法已入 CONTRIBUTING「提交前建议」；lint 基线保持 2 个既有告警，434 用例全绿。

### 4.38 双主题整页视觉回归（round 34）

- 30 余轮度量驱动修复后的"新眼睛"全表面扫描：**28 张截图**（14 表面 × 双主题）——
  登录、列表、批量条、右键菜单、代码预览、历史对话框、视图面板、网格、令牌、审计、
  用户管理、通知面板、移动列表、移动「更多」菜单。逐张目检 12 张关键面（每类表面
  至少一张 × 双主题）。
- **目检结论：零视觉缺陷** ✓（度量驱动的修复在视觉上全部立得住）。顺带目检确认的
  成果：历史对话框 33/67 分栏可见、菜单条目圆角/字重统一、玻璃预览箭头、分段控件
  active tint、「更多」菜单 `is-current` 标识、深色 tonal 浮层、错误 toast 样式
  （「名称未变化」「目标路径已被占用」——截图脚本自身冲突的副产品，恰成证据）。
- 证据集存 `/tmp/vf-shot/ui-r74/`（28 张 PNG，含未逐张细审的 16 张全景）——
  大二进制不入库（dist/release 同规），以路径与本表作为证据引用。
- 过程记录（两条）：
  1. 截图脚本首轮崩在铃铛选择器——**工具页顶栏无铃铛**（管理页布局不同），回主文件页
     截取 ✓；预览管线第 5 次出现"图序 ≠ 调用序"（按内容识别为准，已在 §2 记为已知现象）；
  2. 脚本造历史的重命名撞名（代码.js 已存在）产生两条错误 toast——误打误撞验证了
     错误 toast 的视觉样式 ✓。
- 前端 434 用例全绿（截图轮无代码变更）。

### 4.37 历史对话框比例细化 + 菜单条目语言统一（round 33）

- **① 历史对话框两栏比例**（布局最后一项挂账）：
  - 原 `1.1fr / 1fr` = **52% / 48%**（实测 462/420）—— 提交列表是**导航**、详情是**内容**，
    主流版本浏览器 = 导航窄（≈33%）/ 内容宽（≈67%）✗ 列表过宽、详情局促；
  - 详情栏 `overflow-y: visible` ✗ 长 diff 会撑高对话框（主流 = 双栏各自滚动）。
  - 修复：`minmax(16rem, 0.5fr) / 1fr`（实测 **294 / 588 = 33% / 67%** ✓）；
    详情栏 `overflow-y: auto; min-height: 0`（独立滚动 ✓）；双栏 `max-height: min(64vh,560px)`
    统一（列表原本已有 ✓）。
- **② 菜单条目语言统一**（`.dropdown-item` vs 右键菜单条目）：
  | | 桌面右键（改前即基准） | 「更多」/账号（改前） | 改后（实测两菜单） |
  | --- | --- | --- | --- |
  | 字号 | 13.12px | **14px** ✗ | **13.12px** ✓ |
  | 内边距 | 8px 10px | **6px 48px 6px 16px** ✗ | **8px 10px** ✓ |
  | 条目圆角 | 6px | **0** ✗ | **8px**（`--vf-radius-sm` 令牌）✓ |
  - **特异性陷阱**（值得记）：Bulma 的 `a.dropdown-item, button.dropdown-item`（0,1,1）
    含 `padding-right: 3em`，压过 `.dropdown-item`（0,1,0）——「更多」三项是 `<a>` 而账号是
    `<div>`，同类不同命；选择器列表补上 `a/button` 同特异度、靠后胜出 ✓。
    （教训：比对 cssText 时勿截断 —— 我一度把规则截到 90 字符漏看了 padding-right。）
  - 右键菜单容器/条目圆角并入 `--vf-radius-sm` 令牌（原硬编码 8/6px）。
- 候选排除记录：拖拽边缘自动滚动**不做**（Chromium 对原生 HTML5 拖拽自带容器边缘滚动，
  自建会双重滚动）。
- 顺手补齐 round 31 `thumbnailSize` prop 的默认值（lint `require-default-prop` 归还
  2 个既有告警）。434 用例全绿、tsc/build 通过。

### 4.36 键盘导航 × sticky 表头滚动缓冲（round 32）

- 近期三连用户反馈都在表格区（悬停透底 / 拖拽失灵 / 列宽膨胀）——本轮**主动扫
  下一个高危点**：`scrollIntoView({block:'nearest'})` 的键盘导航与 sticky 表头的冲突。
- **复现**（40 文件列表，↓25 次再 ↑15 次）：活动行顶 **195 < 表头底 233** =
  **藏进表头 38px** ✗✗ —— 用户按 ↑ 选中行直接消失在表头后面（经典 sticky+scroll 缺陷）。
  向下导航无此问题（`nearest` 贴底缘 ✓）。
- 修复：滚动容器 `.desktop-list-shell` 加 **`scroll-padding-top: 2.375rem`**（= 表头实测
  高 38px，rem 随缩放）——`scrollIntoView` 会停在缓冲线之下（sticky 表头的规范解）。
- 验证（同一复现路径）：
  | 场景 | 行顶 / 表头底 | 藏表头？ |
  | --- | --- | --- |
  | 向上导航（↓25 ↑15） | **233 / 233**（恰好停在表头下缘） | **false** ✓ |
  | 向下导航 | — | false ✓ 无回归 |
- 顺手补 `cardMinWidth` 单测（3 断言：144→194、200→270、100→135）。
  前端 **62 文件 / 434 用例**全绿、tsc/build 通过。

### 4.35 列宽默认态修复（用户反馈插曲，位于 round 31 之后）

- 症状（用户报告）：**修改时间/类型/大小等列默认变得很宽、全乱套**。
- 根因：§4.31 的 `table-layout: fixed` 后，所有 `<col>` 都写死宽度，列宽总和 < 表宽时
  浏览器把**余量按比例灌进每一列**（我在 §4.31 注记过 ±3% 机制）——列表区越宽
  （关详情面板/大屏），时间/类型/大小被撑得越宽 ✗。
  另发现 store 初始 `columnWidths` = 全量默认值，"默认自适应"根本无从谈起。
- 修复（主流模型 = **名称列弹性吸收余量**）：
  1. 名称列**默认不写宽度**（`width: auto` 吸收余量），仅当用户拖过才写死；
     时间/类型/大小/勾选/操作列**精确写死**（不再被比例撑宽）；
  2. store 初始 `columnWidths` 改为空对象（类型同步 `Partial<Record<…>>`）；
  3. **复位语义升级**：`setColumnWidth` 在"值=默认"时**删除存储键** →
     名称列双击复位 = 回到自适应（`resetColumnWidth` 自动获得该语义）；
     钳制 `clampColumnWidth` 保留（首次替换时曾丢失，已修复并有测试守护）。
  4. 测试随行为更新（初始/复位断言 → `toBeUndefined`）。
- 验证（真实鼠标拖拽，40+ 场景）：
  | 状态 | 渲染宽（勾选/名称/时间/类型/大小） | 列样式 |
  | --- | --- | --- |
  | 默认 | [40, **342**←吸收余量, **150 ✓精确**, **130 ✓**, **96 ✓**] | 名称 auto |
  | 拖时间 +20 | [40, 322, **170 ✓精确**, 130, 96] | 时间写死 ✓ |
  | 拖名称 +60 | [40, **380 ✓精确**, 170, 130, 96] | 名称写死 ✓ |
  | 双击名称复位 | [40, 322, 170, 130, 96] | 名称**回 auto** ✓ |
  **时间/类型/大小在任何宽度下都精确**（痛点根除）✓。433 用例全绿。
- 过程注记：`/tmp` 夹具目录在本轮中途被宿主清理（服务器与目录消失）——
  重建临时实例后验证，工作区数据无损。

### 4.34 网格骨架几何对齐 + 表单同族复核（round 31）

- **① 网格骨架列几何**（round 27 的高度对齐遗留的列宽尾巴）：
  - 真实网格 = `minmax(var(--file-card-min, 190px), 1fr)`，而 `--file-card-min =
    round(thumbnailSize × 1.35)` **只设在 FileGrid 根上**（默认 144 → **194px**）；
  - 骨架 = 硬编码 `minmax(170px)` ✗✗ —— 默认就 170≠194，**缩略图尺寸变化时列数都会分叉**
    （如 380px 容器：真实 1 列 / 骨架 2 列）→ 跳动。
  - 修复：公式抽为共享 **`cardMinWidth()`**（fileView.store 导出，双端共用防漂移），
    FileSkeleton 增加 `thumbnailSize` prop 自设同名变量（FileBrowser 两处传值）。
  - 验证：骨架与真实网格 **`--file-card-min` 同为 194px ✓、gap 同 14px ✓、列数同为 2 ✓**
    （残余 2px 为加载期滚动条槽位的容器瞬时差，与列宽参数无关）。
- **② 注册/找回表单同族复核**：三模式（登录/注册/找回）**全合规** ✓ ——
  字段高 59px 均匀、输入 35px/14px/400、标签 12.48px/600、提交按钮 36px/13.12px/500
  （AuthShell + 共享 auth 类 ✓ 无分歧）。
- 433 用例全绿、tsc/build 通过。

### 4.33 代码高亮与浮层语义色终检（round 30）

- 三项长期挂账的审计一次闭环（**全部合规、零修复**——本轮价值 = 首次给出数据）：
  1. **`code-theme.scss` 首次对比度审计**（此前一直以"有意配色"豁免）：
     造含注释/字符串/关键字/数字/类名的 JS 文件进文本预览，逐 token 实测对代码底色——
     | token | 浅色（底 rgb(246,248,250)） | 深色（底 rgb(21,24,30)） |
     | --- | --- | --- |
     | comment（经典风险点） | **4.52** ✓ | **5.06** ✓ |
     | string | 12.03 ✓ | 11.57 ✓ |
     | keyword | 5.03 ✓ | 7.05 ✓ |
     | number | 7.13 ✓ | 9.14 ✓ |
     | title | 4.74 ✓ | 9.13 ✓ |
     **双主题全 AA** ✓ —— "有意配色"的豁免实至名归（现首次有数据背书）。
  2. **语义色在深色 overlay 上的终检**（round 17 后挂账）：右键菜单（raised
     `rgb(39,44,52)` = 18%）全部条目对比度 **≥5.74**（删除 danger 5.74 ✓）✓ AA。
  3. **真图上的玻璃箭头目检**（round 28 补 blur 后未在真图目检）：造 4×4 PNG 真图
     进预览 —— 图片加载 ✓、箭头 `blur(8px)` ✓、截图 `ui-r69/arrow-over-image.png` ✓。
- 433 用例全绿、lint 通过。

### 4.32 fixed 布局回归核查 + SkeletonList 宿主对齐（round 29）

- **① 列宽修复（§4.31）的回归核查**：`table-layout: fixed` 的高危副作用 = 长文本不再
  被 auto 布局撑开。实测 47 字长文件名：名称单元格 330px / 文本盒 288px，
  `text-overflow: ellipsis` + `nowrap` + `overflow: hidden` ✓ 行/表零溢出 ✓
  —— **截断行为全合规**，fixed 化无回归。
- **② SkeletonList 行高对齐宿主**（round 27 遗留项）：骨架行统一 22px，而各宿主真实行
  高差异巨大（1.7–5× 跳动）：
  | 宿主 | 真实行 | 骨架（改前） | 跳动 |
  | --- | --- | --- | --- |
  | 版本历史提交卡 | **111px** | 22 | ✗✗✗ |
  | 侧栏概览最近行 | **50px** | 22 | ✗✗ |
  | 转移所有权用户行 | **40px** | 22 | ✗ |
  | 移动对话框目录行 | **38px** | 22 | ✗ |
- 修复：`SkeletonList` 增加 **`rowHeight` prop**（`min-height` 绑定到 `.skeleton-row`），
  四宿主传实测值；侧栏 `rows` 3→5（骨架总量贴近真实面板）。
- 验证（延迟各宿主 API 逐个抓取加载态）：sidebar **50** ✓、history **111** ✓、
  transfer **40** ✓、move **38** ✓ —— **4/4 精确命中** ✓。433 用例全绿、lint 通过。

### 4.31 列宽拖拽修复（用户反馈插曲，位于 round 28 之后）

- 症状（用户报告）：拖动表头分割线调列宽不正常。
- 逐层实证（真实鼠标事件）：
  1. **接线正常**：拖 +60px → `<col>` 的 inline style 320px → 380px（精确）✓
  2. **根因 = `table-layout: auto` 吸收宽度变化**：auto 布局把名称列撑满表格
     （渲染 506px vs style 320px），拖拽数值全被吸收 → 视觉零变化。
- 修复：
  1. `.file-list-table` 加 **`table-layout: fixed`**（`<col>` 宽度成为权威值；
     余量规则 = 列宽总和小于表宽时按比例分配、超出时精确生效 + 容器横滚——
     这是 CSS 规范行为，与主流一致）；
  2. fixed 布局暴露的次生问题：**勾选/操作列（auto）被挤到 2–8px** ✗
     → 定宽 `.file-list-col-check { width: 40px }` / `.file-list-col-actions { width: 96px }`。
- 验证（真实鼠标拖拽，单元格 rect 无歧义测量）：
  | 状态 | cells（勾选/名称/时间/类型/大小） | 勾选框 |
  | --- | --- | --- |
  | idle | [41, 330, 154, 134, 99] | ✓ 可见 |
  | 拖拽 +80 | [40, **400**, 150, 130, 96]（**精确 +80**） | ✓ |
  | 双击复位 | [41, 330, 154, 134, 99]（**完全回到 idle**） | ✓ |
  键盘 ←/→ 微调 ±16 ✓（实测 320→352）；窄屏（1024）列表容器 `overflow: auto`
  横向滚动 ✓ 页面不溢出 ✓（Drive 同款行为）。433 用例全绿、lint 通过。

### 4.30 半透明令牌误用清查（round 28）

- 由 §4.29 用户 bug（半透明 tint 被当独立背景）引出的**同类风险全仓清查**：
  70 处 tint/软底背景 → 交叉脚本定位（同一规则块含 `position: sticky|fixed|absolute`）
  出 **4 处浮层风险**：
  | 浮层 | 悬浮于 | 判定 |
  | --- | --- | --- |
  | `.file-actions`（移动浮动操作栏） | 行内容 | ✓ 有意毛玻璃（`backdrop-filter: blur(8px)` + 强 translucent）保留 |
  | `.preview-arrow`（预览翻页） | **图片** | ✗ 半透明无 blur = 纯透底 |
  | `.file-card-check`（缩略图勾选块） | **缩略图** | ✗ 同上 |
  | `.file-card-menu-trigger`（悬停 ⋯） | **缩略图** | ✗ 同上 |
  | `.app-bar-bell-badge`（未读角标） | 不透明顶栏 | ✓ 叠色语义正确（round 12 已验） |
- 修复：**玻璃令牌必须配毛玻璃** —— 后三者补 `backdrop-filter: blur(8px)`，
  与 `.file-actions` 同语言（主流翻页箭头/勾选块 = 玻璃胶囊 ✓）。
- 验证（真实浏览器，含条件渲染态）：
  | 浮层 | backdrop-filter |
  | --- | --- |
  | `.file-card-check`（Ctrl+A 选择态渲染） | **blur(8px)** ✓ |
  | `.file-card-menu-trigger` | **blur(8px)** ✓ |
  | `.preview-arrow` | **blur(8px)** ✓ |
  | `.file-actions` | **blur(8px)** ✓ |
  探针注记：`.file-card-check` 是条件渲染（无选择态时 DOM 不存在），首次测得 null
  是"无元素"而非"无样式"。433 用例全绿、lint 通过。
  另修正 §4.29 标注为"用户反馈插曲"（与 §4.10 体例一致）。

### 4.29 表头悬停透底修复（用户反馈插曲，位于 round 27 之后）

- 症状（用户报告）：桌面文件列表**表头悬停时背景变透明**，能看见滚动到其下方的文件行。
- 根因：`FileList.vue` 的 `thead th:hover { background: var(--vf-surface-hover) }` ——
  `surface-hover` 是 **6% 半透明 tint**（深色 14% 同理），本意是**叠在实底上的行悬停色**；
  悬停时它换掉了 round 3 sticky 表头规则的**不透明** `--vf-surface` → 表头透明、内容透出。
- 修复：表头悬停改用**不透明** `--vf-surface-sunken`（浅 rgb(245,248,252) / 深 rgb(37,41,49)，
  主流列头悬停 = 浅灰实底）；行悬停的 tint 用法不动（叠色语义正确）。
- 验证（真实浏览器，40 文件列表滚动 400px 后悬停表头）：
  | 主题 | 悬停背景 | alpha |
  | --- | --- | --- |
  | 浅色 | rgb(245,248,252) 实底 ✓ | **无** ✓ |
  | 深色 | rgb(37,41,49) 实底 ✓ | **无** ✓ |
  433 用例全绿、lint 通过。

### 4.28 骨架屏与真实内容尺寸对齐（round 27）

- 防布局跳动（CLS）审计：骨架屏独立实现，行高/卡高从未与真实内容对齐过。
  用**路由延迟制造真实加载态**实测（Playwright 拦截 `files` 接口延迟 2.5s 后逐帧采样）：
  | 形态 | 骨架 | 真实 | 跳动 |
  | --- | --- | --- | --- |
  | 列表行 | **42px** | **48px** | ✗ 每行 6px |
  | 网格卡 | **156px** | **200px** | ✗ 每卡 44px |
- 修复（`FileSkeleton.vue`）：`.file-skeleton-row` 加 `min-height: 3rem`（= 真实行高 48px）；
  `.file-skeleton-card` 加 `min-height: 12.5rem`（= 真实卡高 200px，min-height 兜底、
  与内部块比例无关）。复测：**列表 48 == 48 ✓ 网格 200 == 200 ✓ 零跳动**。
- 过程教训（两条，均已成探针常识）：
  1. **Playwright glob 的 `*` 不跨 `/`** —— `**/api/files*` 漏匹配 `/files/list`，
     导致此前多次"骨架没出现"结论**全部无效**（拦截根本没生效）；改用 handler 内
     `url.includes('/files')` 判断才真正延迟；
  2. 多组件都有 `skeleton-*` 类，选择器先后命中别的组件（SkeletonList 行 22px 误当
     FileSkeleton 行）——量尺寸必须用**无歧义单一选择器**。
- 验证：433 用例全绿、tsc/build 通过。

### 4.27 批量操作条审计与开发约定落档（round 26）

- **批量操作条（BatchActionBar）双主题实测**：
  - 颜色**全对** ✓（round 17 红利：muted #566a7e/5.59、danger #cc0f35/5.7；深色 5.6/5+ ✓）；
  - 框体设计自洽 ✓（`实底 surface + accent-soft 渐变叠加`——注释明确"避免半透明滚动透字"，
    accent-soft-strong 边框 + shadow-menu；深色下色调可读 ✓）；
  - **唯一偏差：7 个按钮全部 30px**（`.desktop-batch-actions .vf-ghost-button` 的
    `min-height: 1.9rem` 覆盖）✗ —— 最后一批未对齐的控件尺寸。删除覆盖 → **36px 控件语言**
    （实测 7/7 = 36px ✓）。
- **开发约定落档（docs/CONTRIBUTING.md）**：
  1. 「本地数据与测试夹具」：CLI 与服务器同 shell 导出 `VFILES_*` 再跑（防 round 23
     漏环境变量写入真实库的事故复发，指向 §4.24）；
  2. 「提交前建议」：`bun run lint:styles` 死样式扫描的用法与豁免说明（脚本首次文档化）。
- 验证：批量按钮 7/7 = 36px ✓；433 用例全绿、tsc/build 通过。

### 4.26 登录页与预览/对话框头部的家族对齐（round 25）

- 审计两个此前未覆盖的表面（登录页 = 首印象、预览面板内部），对照家族语言实测出五处偏差：
  | 项 | 改前 | 判定 | 改后（实测） |
  | --- | --- | --- | --- |
  | 登录卡内边距 | 20.8/20/22.4px | ✗ 节奏微差 | **17.6/19.2/20.8px**（round 4 卡片节奏 ✓） |
  | 登录输入字号 | 13.76px（0.86rem 奇值） | ✗ | **14px**（0.875rem 表单语言 ✓） |
  | 登录按钮 | 14.4px / 37.6px | ✗ | **13.12px / 36px**（控件语言 ✓） |
  | 预览代码块 `pre` | 圆角 0 | ✗ | **8px**（`--vf-radius-sm` ✓） |
  | 对话框头部 `.modal-card-head` | Bulma 默认 **32px / 12px** | ✗ | **20/24px + 14px 顶圆角** ✓ |
  登录卡框体本就合规（14px / border-weak / shadow-card ✓ 三处是唯一真偏差）；
  登录大标题 25.6px/700 保留（主流登录页大标题层级 ✓ 非偏差）。
- 复审（真实浏览器逐项实测）：上表"改后"列即复测值，五项全部对齐 ✓；
  浅色登录页截图无破版；433 用例全绿、无控制台报错。
- 说明：对话框头部是**全局**调整（所有对话框统一受益），mobile-compact 变体有后续
  媒体规则覆盖其内边距 ✓ 不受影响。

### 4.25 树目录落点提示（round 24）

- 补完拖放故事（rounds 10–13）的最后一块：列表行/网格卡片已有「移动到「名称」」chip，
  **树条目只有高亮没有目标标识**。复用全局 `.desktop-drop-hint`（round 13 语言），
  在 `DirectoryTree` 的条目内按 `dragOverPath` 渲染 chip（树无行尾「⋯」，右距收紧 0.35rem）。
- **顺带解开 round 10 的遗留谜团**：当时树的 `is-drag-over` "没触发"被略过——真因是
  **探针竞态**（非应用 bug）：`onDragOver` 有 `if (!props.dragging) return;` 守卫，
  而 `dragging` prop 要等 Vue 下一拍更新，dragstart 与 dragover 同拍派发时守卫拦下；
  真实拖拽事件跨时间轴无此问题。探针在 dragstart 后等 250ms 再派发 dragover 即稳定复现。
- 验证（真实浏览器 + 单测）：
  | 检查项 | 实测 |
  | --- | --- |
  | 拖入树条目 | chip「移动到「项目库」」+ `is-drag-over` 高亮 ✓ |
  | 拖走 | chip 消失 ✓ |
  | 控制台 | 零报错 ✓ |
  新增单测（`DirectoryTree.test.ts`：`dragging` prop + dragOver/dragLeave 生命周期），
  前端 62 文件 / **433** 用例全绿。至此拖放四要素齐备：落点高亮 / 源项变淡 / 目标标识（行、卡、树）/ 完成 toast。

### 4.24 操作反馈文案上下文化（round 23）

- 交互轮：拖放移动的反馈链路实测**已存在**（toast + 铃铛 + 列表刷新 ✓），但对照主流
  （Drive 的 "Moved『x』to『y』"）**toast 缺上下文**——不知道是哪个文件、去了哪里。
- 四处成功 toast 上下文化（`useMoveDialog.ts` / `FileBrowser.vue`）：
  | 操作 | 改前 | 改后（实测捕获） |
  | --- | --- | --- |
  | 移动（拖放/对话框共用） | 目录/文件移动成功 | **已移动「移动件.txt」到「目标目录」** |
  | 重命名 | 目录/文件重命名成功 | **已重命名为「终验.bin」** |
  | 删除 | 目录/文件删除成功 | **已删除「终验.bin」** |
  | 收藏 | 已加入/取消收藏 | **已取消收藏「归档目录」** |
  批量移动 = `已移动 N 个项目到「目标目录」`；目标名取规范化目标路径末段，根目录 → 「根目录」。
  测试无文案断言 → 432 用例不变全绿 ✓。
- **过程事故（完整披露）**：一次 `vfiles import` 在新 shell 中**丢失了 VFILES_* 环境变量**，
  误将 `移动件.txt`/`目标目录` 写入**用户真实存储**（repo `data/`，admin/default 命名空间）。
  发现后：sqlite 查 `entries` 表定位 2 行 → 删除（`entry_versions` 有 ON DELETE CASCADE 自清）
  → 复核 COUNT=0、`vfiles check` 全绿 ✓。残留 2 个孤儿 blob（内容寻址、无引用、无害，
  `maintenance` 可回收）。教训：fixture 操作必须与服务器同 shell 导出环境变量后再跑 CLI。
- 另两条探针教训：toast 选择器须含 `.notification`（Bulma 结构），以及逐步打印防止
  崩溃丢中间结果（本轮重犯一次后改用逐步日志）。

### 4.23 死样式扫描脚本固化（round 22）

- 三批人工清理（rounds 14/15/21）+ round 21 的孤儿 `}` 事故 → 固化为
  **`client/scripts/check-dead-styles.mjs`**（`bun run lint:styles`），检查三类：
  1. **死类**：样式定义但全仓库（模板/脚本/测试）零引用；
  2. **死 keyframes**：定义但无 `animation(-name)` 引用；
  3. **花括号不配平**：样式块 `{`/`}` 计数不等（round 21 事故的护栏）。
- 误报豁免（§4.16 方法论 + 新发现）：
  - Vue `<Transition name="X">` 运行时类（**仅当 `name=` 真实存在**才豁免其后缀族）；
  - 模板/脚本里 `` `...${...}` `` 与 `'prefix' + x` 拼接类的**前缀族**（is-${...} 等）；
  - **highlight.js 运行时类**（`hljs-*` 前缀 + `function_`/`class_`，代码块语法高亮由
    JS 生成、源码无字面量）；
  - 样式注释（`/* */` 与 `//`）需剥离——bulma.scss 注释里的 `bulma.min.css` 曾误报 `.min`；
  - `ALLOW` 文档化保留集（当前为空）。
- **脚本首跑即回本，真发现 ×6**（人工三批全漏）：
  - `@keyframes admin-shimmer`（AdminUsers）、`@keyframes shares-shimmer`（SharedLinks）
    —— 死 keyframes 类此前从未被查过；
  - **`fade-*` ×4（App.vue）** —— round 15 曾按"transition 运行时类"豁免，**实为误判**：
    全仓根本没有 `<Transition name="fade">`；脚本要求 `name=` 存在才豁免，反向纠错了人工结论。
  均以**配平删除**（round 21 教训）移除。
- 验证：
  - 当前代码库 **通过**（722 个类、9 组 keyframes、transition 豁免 2 组（notification/slide-up 真实）、
    动态前缀豁免 5 个）；
  - **负向测试 4/4 检出**（植入死类 ×2、死 keyframes ×1、孤儿 `}` 不配平 ×1 → exit 1）→ 还原后通过；
  - lint/tsc/build/**432 用例**全绿。用法：`bun run lint:styles`（独立于默认 lint，按需/CI 挂载）。

### 4.22 窄桌面响应式与骨架屏深色复核（round 21）

- **宽对话框 × 窄桌面**（1024/1100/1280 × 720 三档实测，历史/移动/预览三个对话框）：
  **全部合规** —— 历史对话框 922→960px（`max-width: 90vw` 钳制在 1024 恰好生效）✓、
  标准对话框固定 640px ✓；三档 × 三框**零横向溢出、零溢出元素** ✓。
  移动端历史 = 行内视图（`actionMode`，非对话框）✓ 结构合理。
- **骨架屏 shimmer 深色复核**（"渐变里的白色字面量"假设）——**被证伪**：
  `SkeletonList`/`FileSkeleton` 渐变均用 `var(--vf-skeleton-shine)`、
  `BatchActionBar` 用 `var(--vf-accent-soft)`，**零字面量**；`--vf-skeleton-shine`
  双主题自适应（浅 `rgba(255,255,255,.65)` 白高光 / 深实测 `#abb1bf38` 灰蓝微光，
  与 border-weak 色系一致）✓；`SkeletonList` 有 `prefers-reduced-motion` 守卫 ✓。
- **死样式第 3 批**：`@keyframes audit-shimmer` **零引用**（仅自身定义）→ 删除。
- 排坑记录：删除正则 `[^}]*` 在嵌套 `to {...}` 的内层 `}` 提前收尾，留下孤儿外层 `}`
  → 构建失败（tsc/测试不解析样式故先行通过）→ 精确修复后重建验证。教训：删嵌套
  CSS 块用非贪婪配平而非字符类。
- 验证：删除后零残留 ✓；tsc/build/**432 用例**/lint 全绿 ✓。

### 4.21 顶栏节奏审计与账户按钮对齐（round 20）

- 布局轮：实测顶栏逐项间距与高度、并闭环 round 16 推迟的"操作条分隔线"项。
- **顶栏节奏审计**（1440px 实测）：右簇（铃铛/主题/账户）间距 **24px 均匀** ✓、
  步进器内 1–2px（附着组，有意）✓、栏高 58px（11+36+11，Linear 48–64 区间内）✓ ——
  唯一缺陷：**账户按钮 32px**，是栏内最后一个高度异类（铃铛/主题均 36）。
- **round 16 的"分隔线待确认"闭环**：`.mobile-bottom-bar` 本就有
  `border-top: 1px solid border-weak` + fixed 定位 + `env(safe-area-inset-bottom)` 内边距
  ——现状**已合规**，无需新增（此前量的是组内行，不是底栏容器）。
- 修复：
  1. `.app-bar-account` `height: 2rem → 2.25rem`（36px，与顶栏控件语言一致）；
  2. 移动折叠菜单的主题/账户行 `2.2rem → 2.5rem`（触屏命中区 ≥40px，原 35.2px 不达标）。
- 验证（真实浏览器，桌面 + iPhone 13）：
  | 上下文 | 铃铛 | 主题 | 账户 |
  | --- | --- | --- | --- |
  | 桌面 | 36 | 36 | **36** ✓（原 32） |
  | 触屏菜单 | — | **40** ✓ | **40** ✓（原 35.2） |
  432 用例全绿、无控制台报错。过程记录：两次脚本断言失败均因锚点漏行（grep 行号
  上下文误读）——修正后写盘，未产生中间态。

### 4.20 分段控件与模式切换细化（round 19）

- 参照 **M3 分段控件 / filter chip 规格**审计三处分段形态：
  | 形态 | 改前 | 判定 |
  | --- | --- | --- |
  | 视图面板 列表/网格 分段按钮 | **28px** 高 | ✗ 小于 M3 下限 32，更小于我们 36px 控件语言 |
  | 用户页角色筛选 chip | **28px** 高 | ✗ M3 filter chip = 32px |
  | 移动「更多」菜单（导航/历史/批量） | 无当前模式标识 | ✗ 状态不可见 |
  分段按钮的 0.15s 背景/文字过渡已存在 ✓（round 1 语言）。
- 修复：分段按钮 **28 → 36px**（对齐 ghost 控件语言）+ padding 0.85rem 节奏；
  筛选 chip **28 → 32px**（M3 chip）；`controls.scss` 新增 **`.dropdown-item.is-current`**
  （accent-soft 底 + accent-text 字 + 600 字重），移动「更多」菜单三项按 `actionMode`
  标注当前模式。
- 验证（真实浏览器，含状态切换）：
  | 检查项 | 实测 |
  | --- | --- |
  | 分段按钮 | 列表/网格 均 **36px**、`padding 0 13.6px`、过渡 0.15s ✓ |
  | 筛选 chip（造 2 用户后渲染） | 全部2 / 管理员1 / 普通用户1 均 **32px** ✓ |
  | 菜单当前态 | 初始「导航」`is-current`（accent-soft/accent-text）✓；切到批量后「批量」跟随 ✓ |
  432 用例全绿、无控制台报错。（筛选条在单用户时不渲染——数据门控，属设计。）

### 4.19 次级面板框体收尾（round 18）

- 对弹层/卡片家族之外的次级面板做最后一轮框体审计（真实浏览器双主题实测）：
  | 面板 | 实测 | 判定 |
  | --- | --- | --- |
  | 历史对话框内部（`.version-history`/`.history-summary*`） | 扁平行、无框 | ✓ 合规（对话框本身是 round 3/10 家族框） |
  | 上传队列（`.upload-queue*`） | 扁平行 | ✓ 合规 |
  | 移动搜索条（`.mobile-search-toolbar`）/ 操作条 | 扁平工具条 | ✓ 合规 |
  | **下载队列（Bulma `.box`）** | **12px 圆角 / 20px 内边距 / Bulma 阴影 / 无边框** | **✗ 唯一异类** |
- 修复：`DownloadQueuePanel` 原本完全没有样式块（纯 Bulma `.box` 默认），补 scoped 样式
  对齐**卡片家族**（round 4 语言）：14px 圆角 + `border-weak` 发丝边 + `shadow-card`
  + 1.1/1.2/1.3rem 内边距节奏。
- 验证（双主题实测）：浅色 白底 / `rgb(238,241,245)` / **14px** / shadow-card /
  **17.6/19.2/20.8px** ✓；深色 `rgb(26,29,35)` / 14% 发丝线 / 14px / shadow-card ✓。
  排坑：探针首次用 `.box` 兜底选择器先匹配到了外层 `file-browser-box`（pad 0 是它的），
  按元素身份复核后确认队列卡片四项全部生效。432 用例全绿、无控制台报错。

### 4.18 Bulma 语义色全面对齐令牌（round 17）

- 深挖第 16 轮"类名逃过字面清查"的线索，对 Bulma 语义类做双主题实测，发现**三层问题**：
  1. **Bulma 1.0 的语义色不随主题变**（浅/深声明同值，hsl 通道设计），且与 `--vf-*`
     是两套颜色：is-danger 按钮是**粉红** `#f14668` 家族（30 处）而我们的红是 `#cc0f35`，
     is-success 薄荷绿 vs 我们的深绿，is-info 天蓝 vs blue-600；
  2. **真实 AA 失败**（浅色）：`has-text-warning` 对白 **1.75 ✗**（搜索框提示文字！）、
     `has-text-danger` **2.8 ✗**（下载错误文字）；
  3. **深色下搜索高亮是刺眼浅色块**（`has-background-warning-light` = 淡桃底 + 黑字，
     与深色主题格格不入）；移动行选中态还在用 Bulma `has-background-light`（与桌面/网格
     的 accent-soft-strong 不一致）。
- 修复（利用 Bulma 的 h/s/l 通道派生设计，每色 4 个变量全家跟随）：
  - `theme.scss` 浅/深色块各覆盖 **danger/success/warning/info 的 4 个通道变量**
    （h/s/l + invert-l），整体对齐 `--vf-*`：浅色 danger `#cc0f35`+白、success 深绿+白、
    warning 深琥珀+白、info blue-600+白；深色按我们"语义色变浅"的哲学（348/86/61、
    160/55/60、35/85/72 配深字，info 蓝-600+白）；
  - `controls.scss` 覆盖 mark 高亮 → `--vf-warning-soft` + 文字随语境；
  - 移动行选中 → `is-row-selected` + accent-soft-strong（与 round 2 语言一致）。
- 验证（真实浏览器双主题，合成探针元素实测）：
  | 浅色 | 实测填充 | 对比度 | | 深色 | 对比度 |
  | --- | --- | --- | --- | --- | --- |
  | is-danger | #cc0f35 = 令牌 ✓ | **5.7** ✓ | | is-danger | 5.82 ✓ |
  | is-success | 深绿家族 ✓ | **5.44** ✓ | | is-success | 11.22 ✓ |
  | is-warning / has-text-warning | #7c4a03 = 令牌 ✓ | **7.54** ✓（原 1.75） | | has-text-warning | 11.08 ✓ |
  | is-info | blue-600 ✓ | **5.17** ✓ | | is-info 白字 | 6.67 ✓（round-1 家族） |
  | has-text-danger | #cc0f35 ✓ | **5.7** ✓（原 2.8） | | has-text-danger | 5.02 ✓ |
  mark：浅色 warning-soft 淡琥珀 / 深色 dim 琥珀（刺眼块消除）✓；
  移动选中 `rgba(37,99,235,.16)` ✓（注意：行有 0.15s 过渡，探针须延迟测量）；
  432 用例全绿、无控制台报错。至此 Bulma 语义层与我们的令牌**同源**。

### 4.17 移动端操作条与文本灰令牌（round 16）

- 首次覆盖移动布局（本目标此前以桌面为主）。审计（iPhone 13 视口实测）结论：
  - 操作条按钮**度量全一致**（高 44px = HIG 触控下限 ✓、图标 18px、字 12px、上传键为主色填充 ✓）；
  - **gap 仅 2.4px（0.15rem）**——相邻触控目标几乎相贴，违背 M3/HIG 的 ≥8px 间距；
  - **Bulma 文本灰工具类逃过了第 12 轮字面色清查**（它们不是字面值而是类名）：
    `has-text-grey`（#7a7a7a）浅色对比 **4.29 ✗**、`has-text-grey-light`（#b5b5b5）**2.0 ✗**，
    共 **11 处 / 4 文件**（加载更多、空态提示、搜索说明、账号标签、历史日期、批量计数、代码行号）。
- 修复：
  1. gap `0.15rem → 0.5rem`（8px），三处实例（导航/历史/批量）共享同一类、一次生效；
  2. `controls.scss` 新增语义文本工具类 **`.vf-text-muted` / `.vf-text-subtle`**，
     11 处按层级替换（`has-text-grey` → muted，`has-text-grey-light` → subtle）。
- 验证（真实浏览器双主题）：
  | 检查项 | 浅色 | 深色 |
  | --- | --- | --- |
  | 操作条 gap | **8px** ✓ | **8px** ✓ |
  | 样例「账号」对比度 | **5.59** ✓ AA | **5.6** ✓ AA |
  | Bulma 灰类残留 | **0** ✓ | 0 ✓ |
  `has-text-grey*` 全仓清零；432 用例全绿、无控制台报错。
  底部分隔线暂不加（模式切换行与操作条同组，边框归属未定——留后续确认）。

### 4.16 死样式全仓扫描收尾（round 15）

- 第 14 轮的引用扫描只覆盖了两个最大文件，本轮扩展到**全部 .vue 组件 + 测试/TS 源**：
  共 **33 个候选**，逐个判别后——
  - **误报保留**：Vue `<transition>` 运行时类 ×12（`fade-*`/`notification-*`/`slide-up-*`，
    由 `name="fade"` 在运行时拼出）；动态拼接类 `is-${variant}`（SkeletonList 的 is-lines）、
    `is-${category}`（存储段 is-audio/video/…）、`is-${status}`（上传 is-done/…）、
    `file-skeleton--${variant}` —— 字面量不在源码里但真实生效。
  - **真死删除（11 条规则 / 6 文件）**：
    Breadcrumb `.path-bar-menu-item`、FileCard `.file-card--shortcut`、
    FilePreviewModal `.preview-state*` ×2、MoveDialog `.move-dialog-state`、
    VersionHistory `.history-detail-empty*` ×3 + `.history-state-empty` + `.history-state-icon`、
    SidebarOverview `.sidebar-skeleton-line.is-short`（加载态早已改用共享 `SkeletonList`）。
- 过程记录：首轮删除脚本因 FileCard 是后代选择器（`.file-card--shortcut .file-card-thumb`）
  断言中断，后续文件未处理——修正选择器模式后补跑，避免了"删一半"的中间态进库。
- 验证：删除后全仓 grep **零残留** ✓；vue-tsc/build/**432 用例**/lint 全绿 ✓。
  被删规则均无引用（结构上不可能渲染），无视觉影响 ✓。

### 4.15 空状态审计与死样式清理（round 14）

- 审计方法：实测四个空状态的度量（图标/标题/说明/内边距/间距）+ 跨文件引用扫描。
- **空状态已统一的确认**（此前没有数据支撑）：文件浏览器空目录、分享、令牌、搜索无结果
  **全部走共享 `EmptyState`，指标逐项一致** —— 图标 34px、标题 15.2px/600、说明 13.12px、
  内边距 52px/16px、间距 4.8px；`.browser-empty*` 自定义实现早已退役但**样式没删**。
- 死样式清理（全仓库引用扫描确认零引用后删除，共 **12 条规则**）：
  - FileBrowser：`.browser-empty` 家族 ×5（含注释）、`.desktop-nav-item` ×3、
    `.desktop-pane-section + …`、`.desktop-pane-heading`、`.desktop-content-pane`、
    `@media` 内的 `.breadcrumb-actions .buttons`；
  - Home：`.hero`（Bulma 覆盖但无 hero 元素）、`.mobile-search-field`（搜索已迁到 MobileSearchBar）。
- 孤儿令牌清理：`--explorer-*` 六个令牌中 **5 个消费者为 0**（历轮重构拆掉后遗留——
  round 10 拆了 panel-bg、本轮拆了 accent-soft），仅保留仍被搜索面板 `::after` 引用的
  `--explorer-panel-border`（并补注释说明）。
- 验证：删除后全仓库 grep 零残留 ✓；视觉回归（真实浏览器）——列表 2 行、工具栏、面包屑、
  目录树、详情面板、空目录 EmptyState「此文件夹为空」全部正常渲染，无控制台报错 ✓；
  tsc/build/**432 用例**全绿 ✓。

### 4.14 拖放目标提示（round 13）

- 交互（拖放体验第三块，rounds 10–12 之后）：高亮说明"哪里可以放"、源项变淡说明"什么在移动"，
  还差"**会移到哪个目录**"——目录行众多且同名结构时，仅靠高亮难以确认目标。
- 实现：`FileItem`（桌面行）与 `FileCard`（网格卡片）在 `dragOver` 时渲染共享 chip
  **「移动到「目录名」」**（`controls.scss` 全局 `.desktop-drop-hint`：accent 描边 + surface 底 +
  accent-text 字，absolute 右侧定位**不产生行内回流**，`pointer-events: none` 不遮挡 drop 事件；
  右侧 2.2rem 起，避开行尾「⋯」按钮区域）。
- 踩坑记录：初次把 chip 插进了 `v-if`/`v-else` 名称链**中间**——`v-else` 会配对到 chip 的
  `v-if`，导致网格卡片名字只在拖动时才显示（编译不报错）。已移到名称链**之外**并加验证断言。
- 验证（真实浏览器，合成 DragEvent）：
  | 状态 | 行 | 卡片 |
  | --- | --- | --- |
  | 静止 | 无提示、名字可见 ✓ | 无提示、名字可见 ✓ |
  | 拖入目录 | chip「移动到「项目」」+ drop 高亮 ✓ | 同 ✓ |
  | 拖走/结束 | chip 消失 ✓ | 消失 ✓ 名字仍显示 ✓ |
  移动端行未加（触屏拖动少且整行名称独占可见）；面包屑/树条目路径本身可见，不重复提示。
  新增 1 个单测（提示出现/消失/名称不回归），前端 62 文件 / **432** 用例全绿，无控制台报错。

### 4.13 组件字面色值清查（round 12）

- 背景：`theme.scss` 开头即约定「组件里的零散色值统一收敛到令牌」，历轮也修过漏网之鱼
  （Bulma `$primary` 的 #2f6db6、hover 的 rgba(47,109,182)）。本轮全量清查
  （theme.scss/code-theme.scss 之外的 `#hex/rgb/hsl` 字面值）：**共 10 处**。
- 分类与修复：
  | 位置 | 判定 | 处理 |
  | --- | --- | --- |
  | 历史版本徽章、移动端按钮 ×3（Home） | 填充面白字 | → `var(--vf-on-accent)`（语义源头，round 1 令牌） |
  | **通知铃铛未读角标**（`danger-text` 底 + 白字） | **真缺陷**：深色 danger-text 为浅粉，白字 ≈2.2:1 | → `danger-soft` 底 + `danger-text` 字 + 加粗 + `danger` 描边（round 7 验证过的语义配对，保持醒目） |
  | 访问令牌页 ×2 | `var(令牌, 字面回退)` | 删回退值（theme.scss 必载） |
  | Loading 白环 | 注释明确「深色遮罩之上保持白」 | **有据可依的例外**，保留 |
  | bulma.scss `$primary/$link` | 令牌源头 | 允许 |
- 验证（真实浏览器双主题实测角标对比度）：
  | 主题 | 改前 | 改后 |
  | --- | --- | --- |
  | 浅色 | （白字/粉底 ~5） | **5.01** ✓（rgb(204,15,53) on rgb(254,236,240)） |
  | 深色 | **≈2.2 ✗** | **5.92** ✓（浅粉字 on soft 粉底） |
  组件内 `#hex` 字面值清零（除 Loading 的注释例外）；431 用例全绿、无控制台报错；
  另核实新登录**不会自动展开**通知面板（对截图疑点做了排除）。

### 4.12 拖起态反馈（round 11）

- 交互（延续第 10 轮拖放反馈）：主流文件管理器拖动时**源项会半透明**（Explorer/Drive 的
  drag lift），此前只有目标侧高亮、源项毫无反馈。
- 实现：`FileItem`（桌面行 + 移动行两个模板）与 `FileCard` 各自维护 `dragging` 本地状态，
  `dragstart` 置位、`dragend` 复位，类 `.is-dragging` → `opacity: 0.55`，
  并把 opacity 并入既有的 `0.15s` 过渡（背景/边框/透明度一起渐入渐出）；
  目录树行不作为拖拽源（不可拖出），故不加。
- 验证（真实浏览器，合成 DragEvent）：
  | 对象 | 拖动中 | dragend 后 |
  | --- | --- | --- |
  | 列表行 td | `is-dragging` ✓ opacity **0.55** ✓ | 类移除 ✓ opacity **1** ✓ |
  | 网格卡片 | `is-dragging` ✓ opacity **0.55** ✓ | 类移除 ✓ opacity **1** ✓ |
  无控制台报错；新增 1 个单测（dragStart 置位/dragend 复位），前端 62 文件 / **431** 用例全绿。

### 4.11 拖拽落点反馈 + 高级搜索面板对齐（round 10）

- **拖拽落点反馈**（交互）：
  - 复核发现桌面行的悬停/落点高亮**没有过渡**（`transitionDuration: 0s`），而移动端行
    （`.file-item`）与网格卡片（`.file-card`）都有 0.15s → 高亮"闪现"；
  - 桌面行 td 增加 `background-color 0.15s`（悬停/选中/落点三层统一渐入），
    全局 `.drop-target` 与树条目（`.is-drag-over`）同样加渐入；
  - 修复规则顺序：`.drop-target > td` 与 `.is-row-selected > td` 同特异性、靠后胜出，
    原先落点色被选中底色压住（实测选中目录行上显示 0.16 而非落点的 0.08）。
  - 说明：`drop-target` **只对目录生效**（文件不是合法落点，`kind !== 'directory'` 直接返回）
    ——这符合主流拖放语义，属设计而非缺陷。
- **高级搜索面板对齐**（外观）：它是工具栏弹层家族的最后一个异类
  （深边框 `--vf-border`、10px 圆角、0.72rem 内边距、shadow-lg、基准面背景），
  且变体块用 `--vf-surface` + `shadow-lg` 覆盖基础规则。
  现统一为家族语言：**border-weak、6px 圆角、12px 内边距、`surface-raised` 背景、菜单阴影**。
- 验证（真实浏览器双主题 + 合成 DragEvent 指向目录行）：
  | 检查项 | 实测 |
  | --- | --- |
  | 落点类/落点色 | `drop-target` 类 ✓、td 背景 = `rgba(37,99,235,.08)`（**选中行上也正确**）✓ |
  | 过渡 | 落点与悬停 td 均 `0.15s` ✓（虚线描边作为模式指示保持即时） |
  | 面板（浅） | 白底 / weak 边框 / 6px / 12px / 菜单阴影 ✓ |
  | 面板（深） | `rgb(39,44,52)` raised 18% / 14% 发丝线 / 6px / 12px / 0.4 菜单阴影 ✓ |
  与视图/排序/上传弹层逐项一致；430 用例全绿、无控制台报错。

### 4.10 下拉与对话框按钮一致性（用户反馈插曲，位于 round 9 之后）

- 反馈：上传按钮旁的下拉菜单式样与其它下拉不同；对话框里的按钮式样不统一。
- 实测定位（双组件度量对比）：
  1. **上传菜单**：包装层 `.desktop-upload-menu` 自带一套框体（边框/圆角 10px/阴影/内边距），
     内层 `.dropdown-content` 又有一套（6px 圆角 + 全局边框/阴影）→ **双层边框、双阴影、
     6/10px 圆角混用**；修复：包装层只留 `min-width/margin-top`，框体统一交给
     round 6 的全局 `.dropdown-content` 规则。
  2. **排序菜单**：`.sort-menu-panel` 覆盖出 `--vf-border`（更深）+ 6px 内边距，
     与上传/账号的 `border-weak` + `8px 0` 不同 → 删除覆盖项，继承全局规则。
  3. **对话框按钮**：移动/分享/转移对话框用 `vf-ghost-button`（36px/胶囊/13.12px），
     而确认框（DialogHost：删除确认、新建文件夹）用 Bulma `.button`（**40px/16px，
     is-link 6px 圆角**）→ 新增 `.modal-card-foot .button` 规则对齐到 36px/胶囊/13.12px/500，
     并在 `pointer: coarse` 块补充页脚选择器保证触屏 40px。
- 复审（同一采集脚本）：
  | 下拉 | 边框 | 圆角 | 内边距 |
  | --- | --- | --- | --- |
  | 上传 / 排序 / 账号 | **全部 border-weak** | **全部 6px** | **全部 8px 0** |
  | 视图（面板型，有意区别） | border-weak | 6px | 12px |
  对话框按钮全部 **36px / 999 胶囊 / 13.12px**（含删除确认与新建文件夹）；
  触屏页脚按钮实测 **40px**；前端 62 文件 / 430 用例全绿，无控制台报错。

### 4.9 状态胶囊跨页统一 + 软背景对比实测（round 9）

- 背景：第 8 轮把徽章统一留到"先造数据"，本轮补上——创建真实分享、令牌与禁用用户后，
  双主题采集四页徽章的度量与**软背景（badge/chip 合成底色）对比度**。
- 对比度复核结论：**全部通过** —— 浅色 6.12–11.34、深色 6.83–9.34，无一项低于 4.5 AA
  （上轮遗留的疑虑解除；状态点类如用户状态/审计结果同样达标）。
- 分歧与统一（沿用第 4 轮"共享类"做法）：三枚胶囊徽章度量不一致——
  | 徽章 | 字号 | 垂直内边距 |
  | --- | --- | --- |
  | tokens-status | 11.84px | **0.8px** ✗ |
  | shares-badge | **11.52px** ✗ | 1.6px |
  | audit-action | 11.84px ✓ | 1.6px ✓ |
  在 `controls.scss` 新增 **`.vf-status-pill`**（inline-flex、0.1/0.4rem 内边距、999 圆角、
  0.74rem 字号、nowrap），三页挂类并删除各自重复声明（颜色/语义态仍由各页保留）。
- 复审（同一采集脚本）：每类**度量组数 = 1**、**三页胶囊跨页统一 True**、
  对比度 6.12–11.34 全部 ≥4.5；状态点模式（用户状态、审计结果）作为另一有原则的形态保留不强行改。
- 测试：前端 62 文件 / 430 用例全绿（tsc/lint/prettier/构建通过）。

### 4.8 accent 文字用色语义修正（round 8）

- 背景：本想复核徽章在软背景上的对比度，审计中先抓到一个家族性问题——
  **23 处（19 个文件）把 `color: var(--vf-accent-strong)` 当文字用**。
  `accent-strong` 是第 1 轮为**填充按钮悬停**设计的深蓝（浅 #1d4ed8 / 深 40% 亮度），
  当文字用在深色主题下变成暗蓝压在深底上。
- 实测失败：用户页激活筛选器的计数文字**深色 2.08:1**（AA 需 4.5）。
- 修正：全部 23 处改用语义正确的 **`--vf-accent-text`**（浅 #1e40af / 深 hsl(217,84%,74%)），
  覆盖 ghost/icon 按钮的激活与悬停态、目录树选中、面包屑、排序/视图选中、对话框强调文字、
  版本历史与上传队列的强调文本、登录页等；`accent-strong` 从此只用于填充背景。
- 复审（同脚本 + 代表元素抽查）：
  | 检查项 | 改前 | 改后 |
  | --- | --- | --- |
  | 用户页筛选计数（深色） | **2.08 ✗** | **7.65 ✓** |
  | 同元素（浅色） | 6.7 | **8.72** ✓ |
  | 目录树 active 文字 | 深色不自适应 | 浅 #1e40af / 深 rgb(133,176,244) ✓ |
  | 登录主按钮（深色填充） | — | rgb(28,82,206) + 白字（6.67 家族 ✓） |
- 说明：徽章**样式**统一（shares/tokens 状态徽章跨页对比）因需先造分享与令牌数据，留作后续轮；
  本轮聚焦已确证的对比度家族性缺陷。
- 测试：前端 62 文件 / 430 用例全绿（tsc/lint/prettier/构建通过）。

### 4.7 文本对比度系统审计（round 7）

- 方法（延续第 1 轮抓到深色强调色 2.83:1 的做法）：浏览器实测**双主题 × 8 个文本令牌 ×
  2 种背景（surface / sunken）** 的 WCAG 对比度，再用真实元素抽查定位典型失败。
- 审计结果：**唯一系统性失败是 `--vf-text-subtle`** ——
  浅色 2.67/2.51、深色 4.14/3.58（全低于 4.5 AA）；其余全部通过
  （muted 4.87、语义色 5.7–7.4、强调文本 8.7 等）。它被 **68 处**使用，
  且全是信息性文本（时间戳、大小、上传队列、搜索位置、历史版本元信息）→ 真缺陷。
- 修复（保持四级层次不倒挂）：
  - 浅色：`muted #627386 → #566a7e`（4.87 → 5.59）、`subtle #90a0b2 → #5f7186`（2.67 → **5.01**）；
  - 深色：`subtle 52% → 60%`（4.14 → **5.54**，sunken 3.58 → **4.79**）；
  - 层次：strong 12.7 / text 10.1 / muted 5.6 / subtle 5.0（浅色）——四档递减且全部 ≥4.5。
- 复审（同一审计脚本）：**双主题 8×2×2 全部 ≥4.5（`双主题全部 ≥4.5: True`，无失败项）**；
  真实元素抽查：白底时间戳「今天 16:24」（subtle 典型用例）现为 5.01 ✓。
- 测试：前端 62 文件 / 430 用例全绿（tsc/lint/prettier/构建通过），截图 `ui-r45/light-text.png`。

### 4.6 深色主题 tonal elevation（round 6）

- 现代深色规范（M3 dark / GitHub dark / Linear）用**色调抬升 + 发丝边框**表达浮层层次，
  阴影只做辅助。审计（浏览器实测深/浅双主题各 overlay 的计算色）发现深色问题：
  | 浮层 | 改前背景（深色） | 问题 |
  | --- | --- | --- |
  | 视图下拉面板 | 10%（**比基准 12% 更暗**） | 浮层"凹陷"，还带 Bulma 白色光晕阴影 |
  | 右键菜单 / 通知面板 | 12%（**与基准同色**） | 层次全靠 0.5 黑重阴影 |
- 改动：
  1. `--vf-surface-raised` 深色 **15% → 18%**（比基准面高一整档），
     `--vf-shadow-menu` 深色 **0.5 → 0.4**（边框接管层次）；
  2. `.dropdown-content`（视图/排序/上传/账号/铃铛等所有下拉）全局接
     `surface-raised + 发丝边框 + shadow-menu`；
  3. 右键菜单（`ContextMenu.vue`）与通知面板（`NotificationCenter.vue`）在各自组件内
     改用 `surface-raised`（全局规则会被 scoped 覆盖——审计中发现，改为组件内声明）。
- 验证（重建后复审计算色）：
  | 主题 | base | toast | dropdown | 右键菜单 | 通知面板 |
  | --- | --- | --- | --- | --- | --- |
  | 浅色 | 白 | 白 | 白 | 白 | 白（**零回归**） |
  | 深色 | 12% rgb(26,29,35) | **18% rgb(39,44,52)** | **18%** | **18%** | **18%** |
  深色截图（`ui-r44/dark-elevation.png`）确认通知面板明显浮于基准面之上、发丝边框清晰；
  430 用例全绿、无控制台报错。

### 4.5 搜索框主流化（round 5）

- 现状（第 1 轮基线）：工具栏搜索输入 **32px**——比 36px 按钮还小，而 Drive/Dropbox 把搜索做成
  页面里最大的控件（40–48px、浅灰填充、胶囊形）；宽度上限 416px 偏保守。
- 改动：
  1. 桌面搜索输入 **2rem → 2.5rem（40px）**、字号 14px，sunken 胶囊填充与 M3 焦点 halo 保留；
  2. 搜索框上限 **416px → 480px**（basis 18rem → 20rem），仍随剩余空间弹性伸缩；
  3. 右侧附着式按钮组随行拉伸到 40px——修复 `.desktop-search-toggle` 固定 36px
     （`vf-icon-button` 的显式 height 阻止 stretch）导致的 2px 高低差；
  4. 触屏（`pointer: coarse`）全局输入 `.input/.select/.textarea` **40px** 命中区。
- 验证（真实浏览器）：
  | 视口 | 输入框 | 搜索框宽 | 附着按钮 | 溢出检查 |
  | --- | --- | --- | --- | --- |
  | 1440×900 | **40px** | 480px | 40/40 对齐 ✓ | 工具栏/页面均不溢出 ✓ |
  | 1280×900 | **40px** | 480px | 40/40 对齐 ✓ | 同上 ✓ |
  | iPhone 13（coarse） | **40px** | — | — | ✓ |
  焦点态：`box-shadow rgba(37,99,235,.29) 0 0 0 ~3px` halo + accent 边框（第 1 轮的焦点语言保留 ✓）；
  截图 `ui-r43/desktop-search-final.png`；430 用例全绿、无控制台报错。

### 4.4 工具页共享外壳（round 4）

- 数据驱动审计（浏览器实测四页卡片的计算样式）发现分歧：
  | 页面 | 圆角 | 内边距 | 标题字重 | 最大宽 |
  | --- | --- | --- | --- | --- |
  | 分享 / 审计 / 用户管理 | 14px | 1.1/1.2/1.3rem | 700 | 1000 / 1200 / 1100 |
  | **访问令牌** | **10px** | **1/1.25/1.5rem** | **600** | 1040 |
- 修复方式（去重而非各页各改）：在 `controls.scss` 新增共享类
  **`.vf-page-card` / `.vf-page-title` / `.vf-page-subtitle`**，四页挂类并删除各自的
  重复 CSS；同时统一最大宽度为 **1200px**（表格页呼吸一致）。
- 复审实测（重建前端后）：四页 **完全一致** —— 圆角 14px、内边距 17.6/19.2/20.8px、
  标题 18.4px/700、宽度 1200px（`四页完全一致: True`）；文件浏览器保持自身外壳
  （14px 圆角与工具页一致，1280px 全宽为设计取舍）。
- 验证：浅/深双主题截图（`ui-r42/tokens-*.png`、`audit-*.png`）无破版、无控制台报错；
  前端 62 文件 / 430 用例全绿（tsc/lint/prettier/构建通过）。

### 4.3 sticky 列头修复 + M3 入场动效（round 3）

- **修复 sticky 列头从未生效**（round 2 验证中发现的既有缺陷）：`.desktop-list-shell thead th`
  规则写在 `FileBrowser.vue` 的 scoped 样式里，而 `<thead>` 由子组件 `FileList.vue` 渲染
  → th 没有 FileBrowser 的 scope 属性，选择器匹配不到，实测 `position: relative`。
  改为 `:deep(thead th)` 后实测 `position: sticky`。
- **入场动效（M3 emphasized `cubic-bezier(0.2, 0, 0, 1)`）**：
  - 下拉菜单（视图/排序/上传/通知等所有 `.dropdown-menu`）：上方 4px 轻移 + 0.97 缩放淡入 0.18s，
    展开方向决定 transform-origin（左锚点 top left / 右锚点 top right）；
  - 行与卡片右键菜单：同族动效 0.16s（元素每次挂载触发）；
  - 对话框 `.modal-card`：下方 10px 轻移 + 0.98 缩放淡入 0.2s；
  - `prefers-reduced-motion: reduce` 下全部关闭。
- 验证（真实浏览器，40 个文件的长列表）：
  | 检查项 | 实测 |
  | --- | --- |
  | sticky 列头 | `position: sticky`（修复前 relative）；滚动 700px 后 `thTop == shellTop == 191`（pinned ✓） |
  | 下拉动效 | `vf-menu-enter 0.18s cubic-bezier(0.2,0,0,1)` ✓ |
  | 右键菜单 | `vf-menu-enter 0.16s`（网格卡片菜单含全部 10 项含「删除」✓） |
  | 对话框 | `vf-modal-enter 0.2s cubic-bezier(0.2,0,0,1)` ✓ |
  | reduced-motion | `animationName: none` ✓ |
  | 功能回归 | 网格切换 ✓、取消对话框后 `is-active` 消失 ✓、无控制台报错 ✓ |
- 测试：前端 62 文件 / 430 用例全绿（tsc/lint/prettier/构建通过）。

### 4.2 列表行状态语言 + 配色迁移收尾（round 2）

- 复核发现两处**配色未随第 1 轮迁移**的真实问题：
  1. Bulma 的 `$primary`/`$link` 仍是旧蓝 `#2f6db6` → 所有 Bulma 组件
     （`.button.is-primary`、复选框选中态、链接）与新强调色 `#2563eb` 不一致；
  2. `--vf-surface-hover` 浅色层还是旧蓝 `rgba(47,109,182,.06)`。
  两者均已迁移；编译产物中旧蓝 `2f6db6` 残留为 **0**。
- 列表行改用 M3 状态层数字 + Drive 圆角行面：
  - 表格 `border-collapse: separate`（collapse 下单元格圆角不渲染）；
  - **悬停 = accent-soft（浅 8% / 深 16%）**、**选中 = accent-soft-strong（浅 16% / 深 30%）**，
    两态相差一倍、肉眼可分（改前 6% vs 8% 几乎无差别）；
  - 悬停/选中行面 **8px 圆角**（首尾单元格圆角），普通行分隔保持直边；
  - 网格卡片选中同步升到 accent-soft-strong，与列表一致。
- 验证（真实浏览器 浅/深双主题实测计算色）：
  | | 浅色 | 深色 |
  | --- | --- | --- |
  | 悬停 | rgba(37,99,235,.08) | rgba(75,122,231,.16) |
  | 选中 | rgba(37,99,235,.16) | rgba(75,122,231,.30) |
  | 悬停行面圆角 | 8px | 8px |
  截图（`ui-r40/light-rows.png`）确认选中/悬停两态清晰、行面圆角正常。
  测试：前端 62 文件 / 430 用例全绿（tsc/lint/prettier/构建通过）。
- 本轮顺带发现（移交 round 3 修复）：列头 sticky 规则写在 FileBrowser 的 scoped 样式里，
  但 `<thead>` 由子组件 `FileList.vue` 渲染 → scope 属性不匹配，**sticky 列头从未生效**
  （实测 `position: relative`）。

### 4.1 控件语言与配色现代化（round 1）

- 基线实测（1440×900）：按钮 32px、图标钮仅 **26.4px**、ghost 圆角 6px 而主按钮是胶囊（不一致）；
  强调色 #2f6db6 偏灰蓝；**深色主题白色按钮文字对比度仅 2.83:1（未达 WCAG AA 4.5）**；
  无任何可见键盘焦点环、无按压反馈。
- 改动（对照 M3/Fluent2/Drive/Dropbox）：
  1. **尺寸**：控件 36px（2.25rem），`pointer: coarse` 触屏 **40px** 命中区（M3/HIG 下限）；
     顶栏历史步进按钮去掉 26.4px 覆盖，统一到控件语言；
  2. **形状**：ghost / 主按钮 / 图标按钮 / 中性 `.button` 统一**胶囊**（M3、Drive、Dropbox 工具栏语言）；
     表单控件 8px（M3 small，6→8）；输入框焦点统一 M3 式 halo（`0 0 0 3px --vf-focus-ring`）；
  3. **状态层与焦点**（M3）：新增 `--vf-surface-pressed` 按压层（ghost/图标按钮 `:active`）、
     全部可交互控件统一 `:focus-visible` **2px accent-text 实线环 + 2px 偏移**；
  4. **配色**：浅色强调色 #2f6db6 → **#2563eb**（Drive #0B57D0 / Dropbox #0061FF / GitHub #1F6FEB 家族，
     白字 5.17:1 ✓），hover #1d4ed8、链接文本 #1e40af；**深色填充面取更深的蓝 #1c52ce，
     白字对比度 2.83 → 6.67:1（修复 AA 违规）**，深色链接用 hsl(217 84% 74%)；新增 `--vf-on-accent` 配对；
     图表「文档」分类色同步。
- 验证（真实浏览器，浅/深双主题 + iPhone 13 触屏）：
  | 指标 | 改前 | 改后 |
  | --- | --- | --- |
  | 按钮高度 | 32px（图标钮 26.4px） | 36px（触屏 40px ✓ 实测） |
  | 按钮圆角 | ghost 6px / 主按钮胶囊 | 统一胶囊 ✓ |
  | 焦点环 | 无 | 2px 实线环（浅色 rgb(30,64,175) / 深色 rgb(133,176,244)）✓ |
  | 白字对比度 | 浅 5.27 / **深 2.83 ✗** | 浅 **5.17** ✓ / 深 **6.67** ✓ |
  | 强调色 | #2f6db6 | #2563eb（深色填充面 #1c52ce） |
  主题一致性另经 DOM 计算色核验（浅色 navbar/卡片 rgb(255,255,255)、深色 rgb(20,22,26)/rgb(26,29,35)）。
- 测试：前端 62 文件 / 430 用例全绿（`vue-tsc`/`eslint`/`prettier`/构建通过）。
