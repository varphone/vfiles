# 客户端 UI 整改：已完成与待办

承接 [CLIENT_UI_AUDIT.md](CLIENT_UI_AUDIT.md)。本文件只记录**尚未完成**的项，以及动手前必须知道的坑。

## 已完成（截至 37429ec）

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

## 待办

### 1. 大列表虚拟滚动
`FileList` 全量渲染，首屏 40 条、每次 +30；`FileItem` 收 20+ props / 18 事件，选中一项会整列表重渲染。

**做法**：独立主题。先做 `shallowRef` + `v-memo`（按 `path + selected + renaming`），再评估 `useVirtualList`；`selectedPaths` 改为父层算好布尔值传入。

### 2. 字号 / 间距 / 断点收敛
38 个具体字号（26 个小于 14px 的 rem 值，0.62–0.86 密集分布）；400 处间距声明直接写 rem，只有 262 处用 `--vf-space-*`；13 个宽度断点，设计文档只定 4 档。

**做法**：建 7 档字号 token（11/12/13/14/16/20/24）与 4 档断点 mixin；新代码禁止裸 rem/px，按页面分批替换，不做一次性大改。

### 3. 拆分 `FileBrowser`
[FileBrowser.vue](../client/src/components/file-browser/FileBrowser.vue) 3700 行，桌面/移动两套模板；`FileItem` 单行 63 个 DOM 节点。选择一项会牵动整块重渲染。

**做法**：按工具栏 / 列表 / 移动布局 / 覆盖层拆组件与 composable；先做选择状态 `shallowRef` + 行 `v-memo`，再和虚拟滚动合并验证。

### 4. 产品能力缺口
删除不可撤销且无回收站、无自助改密与个人资料、分享链接只有有效期（无密码/权限）、通知历史仅内存、统一搜索固定 20 条且无「查看全部结果」。

**做法**：按数据安全优先级排：回收站/撤销 > 自助改密 > 分享密码/权限 > 通知持久化 > 搜索分页。前三项需要后端接口。

### 5. 无障碍与视觉回归进 CI
本次键盘问题说明点击测试抓不到：594 个用例全绿，但聚焦按钮按 Enter/Space 仍被全局快捷键吞掉。

**做法**：加三类别回归——Playwright 键盘冒烟（按钮 Enter/Space、对话框焦点归还、菜单方向键）、`@axe-core/playwright` 扫描主要页面、`ui:sweep` 截图基线自动比对。

## 提交前的固定检查

```bash
cd client
bun run check          # eslint + 死样式 + vue-tsc + vitest
bun run build && bun run size:check
bun run ui:sweep -- --out /tmp/vfiles-sweep
```

样式类改动**必须**看截图：jsdom 不应用 SFC 样式，测试绿不代表样式没坏（见 `c0113f7` 的教训）。
