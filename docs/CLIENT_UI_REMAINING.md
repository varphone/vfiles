# 客户端 UI 整改：已完成与待办

承接 [CLIENT_UI_AUDIT.md](CLIENT_UI_AUDIT.md)。本文件只记录**尚未完成**的项，以及动手前必须知道的坑。

## 已完成（截至 f669057）

- **功能**：移动端批量条（ref 解包）、缩略图 watcher 泄漏、选择态跨目录泄漏、确认框 Promise 悬挂、批量删除部分失败汇总、批量下载走队列。
- **可访问性**：页面缩放、对话框 `role/aria-modal/aria-labelledby`、Esc 只关最上层、Tab 焦点回收、4 个输入框标签、辅助文本对比度达 AA（浅 4.99:1 / 深 5.76:1）。
- **移动端**：`--vf-z-*` 七级层叠、iOS 安全区（底栏/抽屉/对话框）、`100dvh` 回退。
- **性能**：首屏三次串行往返改并行、`loading` 与 `mutationPending` 拆分、体积预算口径修正（原统计把 `.gz/.br` 副本再压一次，虚高约 3 倍）。
- **结构**：`useListResource`（追加式）、`usePagedResource`（页码式）；活动/收藏/分享/访问令牌已迁移；审计与用户管理加本地序号防护；删除 1091 行死代码。
- **样式**：主按钮填充回归修复、全站表头几何统一（44px + 垂直居中 + 13px 灰字）。

## 待办

### 1. `modern.scss` 重复块去重（约 80 行）
第 6–88 行与第 163 行之后各有一份 `.button` / `.input` / `.table`。后者生效（导入顺序靠后）。

**坑**：两份**并非逐字相同**。第一次清理时用正则整块删除 `.vf-primary-button`，连带删掉了 `.button.is-primary` 的声明块，导致主按钮只在 hover 时才有蓝色填充（已在 `c0113f7` 修复）。
**做法**：逐条对比属性，只删被后者完全覆盖的声明；删完 `bun run lint:styles` + `bun run ui:sweep`，**打开截图看主按钮、输入框、表头**再提交。

### 2. 六张表统一挂 `.table`
`SystemInfo` / `AccessTokens` / `AccessPasswords` / `RecentView` / `FavoritesView` / `SharedLinks` 各抄 11 行表头与单元格规则，已出现 `13px` vs `0.8125rem` 漂移。

**做法**：给表格加 `class="table ..."`，删掉各自的 `th`/`td` 规则；确认粘性表头、列宽（`table-layout: fixed`）、`aria-sort` 不受影响。

### 3. 卡片头 / 空态 / 徽标归并
- 卡片头有 5 种 `gap`（`.5rem`/`.55rem`/`.6rem`/`1rem`），`SectionCard` 与 `Drawer` 又各一套；
- 空态 5 套 padding（`EmptyState` 之外还有 4 处自绘）；
- 徽标 5 套尺寸（`Badge`、`.vf-status-pill`、`CommitItem`、`.scope-badge`、通知计数）。

**做法**：以 `SectionCard` / `EmptyState` / `Badge` 为准，其余改为复用组件或对齐令牌。注意死样式检查会报未使用的类。

### 4. `controls.scss` 与 `modern.scss` 的控件规则合并
同一控件在 `controls.scss:268`（8px 圆角）与 `modern.scss:36/207`（3px）各写一份，导入顺序决定胜负，注释与实际相反；焦点样式也有 `box-shadow` 与 `outline` 两套。

**做法**：保留 `modern.scss` 的 3px 与 halo 焦点，删 `controls.scss` 的重复块与其注释。

### 5. 大列表虚拟滚动
`FileList` 全量渲染，首屏 40 条、每次 +30；`FileItem` 收 20+ props / 18 事件，选中一项会整列表重渲染。

**做法**：独立主题。先做 `shallowRef` + `v-memo`（按 `path + selected + renaming`），再评估 `useVirtualList`；`selectedPaths` 改为父层算好布尔值传入。

## 提交前的固定检查

```bash
cd client
bun run check          # eslint + 死样式 + vue-tsc + vitest
bun run build && bun run size:check
bun run ui:sweep -- --out /tmp/vfiles-sweep
```

样式类改动**必须**看截图：jsdom 不应用 SFC 样式，测试绿不代表样式没坏（见 `c0113f7` 的教训）。
