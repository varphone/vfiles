# 客户端 UI 整改：已完成与待办

承接 [CLIENT_UI_AUDIT.md](CLIENT_UI_AUDIT.md)。本文件只记录**尚未完成**的项，以及动手前必须知道的坑。

## 已完成（截至 6485dc6）

- **功能**：移动端批量条（ref 解包）、缩略图 watcher 泄漏、选择态跨目录泄漏、确认框 Promise 悬挂、批量删除部分失败汇总、批量下载走队列。
- **可访问性**：页面缩放、对话框 `role/aria-modal/aria-labelledby`、Esc 只关最上层、Tab 焦点回收、4 个输入框标签、辅助文本对比度达 AA（浅 4.99:1 / 深 5.76:1）。
- **移动端**：`--vf-z-*` 七级层叠、iOS 安全区（底栏/抽屉/对话框）、`100dvh` 回退。
- **性能**：首屏三次串行往返改并行、`loading` 与 `mutationPending` 拆分、体积预算口径修正（原统计把 `.gz/.br` 副本再压一次，虚高约 3 倍）。
- **结构**：`useListResource`（追加式）、`usePagedResource`（页码式）；活动/收藏/分享/访问令牌已迁移；审计与用户管理加本地序号防护；删除 1091 行死代码。
- **样式**：主按钮填充回归修复、全站表头几何统一（44px + 垂直居中 + 13px 灰字）、`modern.scss` 控件/数据表重复块合并（f100431：只删被后者覆盖的声明，前一份独有的并入生效块；34 个控件的基准与 hover/焦点计算样式不变，52 张 ui:sweep 截图无样式差异，仅时间戳与 toast 等动态内容不同）。
- **键盘与可访问性**：文件页全局快捷键不再吞掉控件上的 Enter/Space（预览关闭时也不再全站捕获 Space）；右键菜单聚焦/方向键/Esc 回归；活动行与选中态分离；对话框滚动锁 + 背景 inert + 焦点归还；面包屑与移动端命中区、AA 对比度、h1、重复 `id`、中英混排一并修正（8ef4289）。
- **导航**：当前目录写入 `?path=`，刷新、分享链接与浏览器前进/后退都停在同一目录（e32a6e4）。
- **表格与控件**：六张手写表格统一挂 `.table`，删除各自的 `th`/`td`/hover/末行规则；`controls.scss` 的 8px 圆角与焦点重复块删除，焦点控件统一 3px + modern halo（220feec）。
- **卡片/徽标/空态**：卡片头间距与内边距收敛到 token，徽标统一 `--vf-badge-*`（标准 12px + 小号），AdminOverview/SystemInfo 空态复用 `EmptyState compact`；表头排序按钮命中区增至 44px，路由切换后焦点落到 main（37429ec）。
- **排印**：38 个具体字号归并为 10 档 `--vf-text-*` token（新增 11/12/13/14/16/18/20/22/24/28px），352 处 / 64 文件替换；52 张截图、50 组宽度×页面溢出检查、对比度/命名审计均无结构变化（5fd71fe）。
- **断点**：400/560/800/1100/1279 五个一次性宽度并入 480/600/768/1024/1280，保留 769/899/1023 配对值；新边界逐档实测，无页面级横向溢出（12083b0）。
- **间距**：190 处精确落在 4px 网格上的 padding/margin/gap 换成 `--vf-space-*`（41 文件），计算样式零差异；0.3–0.6rem 光学校准带按设计令牌决策保留（b963d72）。
- **通知与搜索**：通知历史/未读写入 sessionStorage，刷新保留、退出登录或会话失效即清；统一搜索满 20 条时提供「查看全部结果」，结果数用 aria-live 播报；快捷键面板补 Ctrl+K（905f3be）。
- **浏览器回归**：新增 `ui:regression`（Playwright + axe），覆盖键盘激活、右键菜单、对话框滚动锁/inert/焦点、目录 URL 前进后退与 10 个页面的 WCAG A/AA 扫描；CI 构建客户端/服务端、安装 Chromium 后执行。首次运行修复列表点击后的焦点上下文、通知/账号可访问名称、系统状态胶囊对比度（edf310b）。随后把深色 + 390px 移动端纳入 axe 扫描，修复移动底栏「更多/首页」与图标返回链接的名称问题（0d8f49f）。回归已扩到 24 项（窗口化列表/网格、勾选、布局基线）。
- **长列表与桌面布局**：桌面外壳锁 `100dvh`，列表/设置/活动改为面板内滚动，列表表头真正 sticky；`FileList` 超过 60 行后只渲染视口窗口 + 上下占位行，150 文件实测 DOM 31 行，End 键可达末行，删除后窗口自动收敛；`ui:regression` 扩到 18 项并覆盖两侧端点与勾选状态（bb04fd0）。
- **选择重渲染**：`selectedPaths` 改为 `shallowRef`（整体替换），`FileList` 行与 `FileGrid` 卡片按全部渲染相关 props 加 `v-memo`；回归增加勾选状态断言（18 项，701b74d）。
- **网格窗口化**：新增通用 `useVirtualWindow`（测量列数/行高、兼容容器与页面滚动、键盘活动项钉住），`FileGrid` 只渲染视口附近的卡片并用上下 padding 撑高；150 文件桌面网格 26–40 张、移动端 6 张，两端可达，回归 20 项（5f82b7d）。
- **个人资料**：`POST /auth/profile` 更新邮箱（主密码会话、格式/唯一性校验、审计）；账号菜单新增「个人资料」对话框，保存后同步本地会话（509bac7）。
- **自助改密**：`POST /auth/password` 校验当前密码、写入新哈希、撤销该账号全部会话并写审计；账号菜单新增「修改密码」对话框（当前/新/确认，≥8 位且不得相同），成功后清本地状态回登录页（d7bc09f）。
- **布局基线**：`ui:regression` 在 1440×900 与 390×844 采集顶栏/侧栏/内容区/工具栏/列表/状态栏/首行等盒子，与 `tests/fixtures/ui-layout.json` 以 3px 容差比较；`UPDATE_LAYOUT=1` 重新生成（95fbfa2）。
- **类型与契约**：开启 `strictTemplates`，修复 73 个模板类型错误；顺带补 `AdminOverview` 的 `SkeletonList` 导入、`MobileSearchBar` 的 `enter-results`/ArrowDown、桌面内容区的重命名绑定，并让 `FileBrowserCollection` 声明/转发完整事件契约（6485dc6）。
- **首屏占位**：`index.html` 增加主题化启动骨架（含 reduced-motion 回退），Vue 挂载后替换，覆盖 Home chunk 加载前的空白窗口（5e71ddd）。
- **构建体积**：删除 `prefers-color-scheme` 的那份深色变量（theme-init 已保证显式 `data-theme`，并补隐私模式回退）；Tabler 图标从 19 个小 chunk 合并为 1 个（47bf5ba）。
- **结构（进行中）**：拖拽光标 chip（状态/目标判定/坐标/监听生命周期）抽为 `useDragChip`（6bbe05e）。所有权转移（状态/上限/提交/失败恢复）抽为 `useFileTransfer`（d155fa5）。上传队列 UI 状态（对话框/选择器/进度胶囊/拖放入队）抽为 `useFileUpload`（1e5cc0b）。条目动作与弹窗状态（删除/新建/移动/分享/历史/详情）抽为 `useFileActions`（ede96c6）。行内重命名（状态/校验/请求/刷新）抽为 `useFileRename`（ccac3e7）。桌面内容区（加载/错误/资料库/空态/搜索/列表与加载更多 slot）抽为 `FileBrowserDesktopContent`（ac32317）。桌面状态栏抽为 `FileBrowserStatusBar`（摘要/提示/选择/定位/快捷键入口，8bf5724）；移动端内容区（加载/错误/资料库/空态/搜索/列表与加载更多 slot）抽为 `FileBrowserMobileContent`（633eb48）。工具栏右侧的版本切换抽为 `FileBrowserHistoryControls`，列表/网格/排序/详情控制抽为 `FileBrowserViewControls`（8fdff03）；根视图「全部文件」菜单抽为 `FileBrowserViewMenu`，父层保留 `viewMenuOpen` 供 Escape 关闭（41d250b）；工具栏左侧的侧栏开关/写入口/范围徽标与面包屑/菜单切换抽为 `FileBrowserToolbarLeading`、`FileBrowserToolbarHeading`（b189c26）。文件浏览器的历史/分享/详情/转移/快捷键/预览覆盖层抽离为 `FileBrowserDialogs`，用显式 props/emits 连接，转移失败路径通过 `finishTransfer` 暴露；新增组件契约测试（1530b9e）。键盘处理与输入/控件判定抽离为 `useFileBrowserKeyboard`，主组件通过依赖注入连接（fc773b7）。目录导航、范围守卫、403/范围回退与滚动记忆抽离为 `useDirectoryNavigation`（88429df）。列表/网格的三处重复接线收敛为 `FileBrowserCollection`（4acd50c）。

## 待办

### 1. 继续拆分 `FileBrowser`
[FileBrowser.vue](../client/src/components/file-browser/FileBrowser.vue) 约 2416 行；覆盖层、键盘、目录导航、列表/网格接线、工具栏、状态栏、移动/桌面内容区、重命名、条目动作、上传、转移、拖拽 chip 均已抽离。剩余主要是拖放落子编排与详情面板接线。

**做法**：下一刀评估拖放落子 composable 或详情面板容器；保持 `FileList`/`FileGrid`/`useVirtualWindow` 的滚动契约与 `useDirectoryNavigation` 的导航单点。

### 2. 产品能力缺口
删除不可撤销且无回收站，分享链接只有有效期（无密码/权限）。

**做法**：按数据安全优先级排：回收站/撤销 > 分享密码/权限；两项都需要后端接口与数据迁移。

### 3. 像素级截图基线（可选）
关键容器的布局尺寸基线已进 CI（21 项、3px 容差）；像素级 diff 仍依赖手动 `ui:sweep`。跨平台字体渲染会让像素 goldens 不稳定，暂不提交基线图。

**做法**：若确需像素级，用固定 Playwright 容器生成 goldens；否则保留「布局基线 + 手动截图」策略。

## 提交前的固定检查

```bash
cd client
bun run check          # eslint + 死样式 + vue-tsc + vitest
bun run build && bun run size:check
bun run ui:regression  # 需要 target/debug/vfiles + client/dist
bun run ui:sweep -- --out /tmp/vfiles-sweep
```

样式类改动**必须**看截图：jsdom 不应用 SFC 样式，测试绿不代表样式没坏（见 `c0113f7` 的教训）。
