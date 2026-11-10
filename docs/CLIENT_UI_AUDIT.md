# 客户端界面审计（Nextcloud 重构后）

审计范围：`client/`（82 个 `.vue`、5 个 `.scss`、127 个源文件），只读检查 + Playwright 实测。
构建基线：`dist` 188 文件 / 2.16 MB；JS 826 KB（gz 278 KB）、CSS 757 KB（gz 95 KB）。
测试基线：`bun run check` 594 用例通过；死样式 1185 类、硬编码色 0 处。

严重度：**P0** 功能已坏 / 明显缺陷；**P1** 影响一致性、可维护性或可用性；**P2** 打磨。

---

## 一、P0（建议本迭代内修完）

| # | 位置 | 问题 | 改法 |
|---|---|---|---|
| 1 | `Home.vue:481-483`、`298-318`、`FileBrowser.vue:2743-2764` | 移动端批量条读的是 `browserRef.value?.batchMode?.value`，但 `defineExpose` 暴露的 ref 已被 Vue 解包，`.value` 恒 `undefined` → 底栏永远「已选 0 项」，批量操作静默无效，「批量」按钮来回横跳 | 去掉 `.value`；用 `InstanceType<typeof FileBrowser>` 替代手写句柄 |
| 2 | `FileCard.vue:389-408`、`37` | 缩略图在 `computed` 内 `ref()` + `watch()`：模板 ref 指向不存在的绑定 → 缓存命中时骨架永久盖住图片；每算一次泄漏一个永不停的 watcher | `thumbImageRef` 与 `watch` 提到 setup 顶层，computed 只算 URL |
| 3 | `FileBrowser.vue:2136-2150`、`useFileSelection.ts:188-190` | 切换目录不清理选择：显示「已选 5 项」但删除/移动在新目录里找不到条目，静默返回 | 导航时 `clearSelection()`；空集时给 `warning` |
| 4 | `index.html:8` | `user-scalable=no, maximum-scale=1.0` 禁止缩放，违反 WCAG 1.4.4 | 去掉这两个参数；输入框字号提到 16px 防 iOS 自动放大 |
| 5 | `ViewsSidebar.vue:192,249-257` + 4 个调用点 | 侧栏搜索框 `emit("search")` 无人监听，回车清空输入且毫无反应（注释自认占位） | 接统一搜索或 `disabled` + 提示 |
| 6 | `Home.vue:651` `z-index:900` vs `Modal.vue:134` `100` | 移动底栏压住对话框遮罩与视图抽屉；全站 z-index 有 18 个散值 | 抽 `--vf-z-*` 七级（sticky/dropdown/scrim/drawer/modal/bottombar/toast） |
| 7 | `Home.vue:662` 与 `818` | iOS 安全区算两遍：底栏自身抬 `env()`，内部 padding 再加一次 → 悬浮 34px 且下方露底色 | 二选一，推荐 `bottom:0` 只由内层吃 inset |
| 8 | `Modal.vue:146-148,189` | 对话框 `90vh` 无 `dvh` 回退，底栏无安全区 → iOS 上按钮被手势区盖住 | 两行写法 + `padding-bottom: calc(var(--vf-space-3) + env(safe-area-inset-bottom))` |
| 9 | `AppLayout.vue:26`、`Home.vue:610`、`AdminLayout.vue:136,146`、`FileBrowser.vue:3561,3568,3683` | `100dvh` 单写、无 `100vh` 回退，旧浏览器整条声明被丢弃 | 统一 `100vh` + `100dvh` 两行（`AuthShell.vue:116` 已是正确写法） |
| 10 | `FilesLayout.vue:246-248`、`Drawer.vue:144` | 移动抽屉底部无安全区，且缺 `overscroll-behavior: contain` | 补 padding 与 overscroll |

## 二、P1 — 交互与可访问性

- **Modal 缺语义**（`Modal.vue:1-13`）：12 个对话框没有 `role="dialog"` / `aria-modal` / `aria-labelledby`；`Drawer.vue` 反而写得对。改一处受益 12 处。
- **Esc 被 5 个 owner 同时处理**（`FileBrowser.vue:1942-1967`、`Modal.vue:95-98`、`Home.vue:432`、`FilesLayout.vue:121`）：全局 15 个 `document keydown` 监听，没有层级栈；弹窗里按 Esc 会顺手清空选择。需要单一分发器 + 栈。
- **焦点可逃出弹窗**（`Modal.vue:12,55-70`）：`@keydown.tab` 绑在卡片上，点标题后焦点落到 body，Tab 从页面顶部开始；背景无 `inert`。
- **快捷键穿透**（`FileBrowser.vue:2775-2785`）：`anyOverlayOpen()` 漏掉详情、转移所有权与全局确认框，弹窗打开时 Delete/F2 仍在操作后台列表。
- **全局确认框单槽位**（`dialog.ts:48-58`）：第二次调用直接覆盖前一个 Promise，前一个 `await` 永不返回 → 悬挂请求。
- **快捷键文档不符**（`KeyboardShortcutsDialog.vue:48-113`）：漏了 `Ctrl/Cmd+K`；「弹窗内不触发」与实现不符；`Ctrl+空格` 与系统输入法热键冲突。
- **4 个无标签输入框**：`DialogHost.vue:26-33`（所有新建/重命名都走它）、`FileBrowser.vue:908/935`、`MobileSearchBar.vue:14-24`。
- **`role="menu"` 子节点不是 `menuitem`**（`Home.vue:47-70`）；`Breadcrumb.vue:5-31` 末级缺 `aria-current="page"`；`AdminUsers.vue:221` 整行可点但没有按钮语义。
- **批量删除/下载缺口**（`useFileSelection.ts:161-215`）：删除无进度、不可取消、首个失败即静默中断；批量下载绕过下载队列，一次扇出 N 个并发。
- **401 可能死循环**（`main.ts:41-46,103-121`）：后端未启用认证时「跳登录 → 守卫弹回首页 → 再 401」的节流只作用于提示文案。
- **只有错误没有重试**：`SystemInfo.vue:21`；**没有骨架屏**：`AdminOverview.vue:20-50`；**静默降级**：`auth.store.ts:65-71`（bootstrap 失败后 `features=null`，分享/FTP 入口消失且无提示）。

## 三、P1 — 结构与重复

| 主题 | 现状 | 建议 |
|---|---|---|
| 上帝组件 | `FileBrowser.vue` 3686 行，模板里塞 12 个弹层、桌面/移动两套布局（317-612 / 614-848 几乎逐块重复） | 拆 8 个组件 + 3 个 composable；两套布局合并为单模板 + 断点 |
| reload 三套标准 | 9 个列表页各写 `reload()`，多数无 seq/abort（快速搜索/翻页可被旧响应覆盖）；`files.store`、`useDirectoryHistory`、`useFileSearch` 做得对 | 抽 `useListResource`（seq + AbortController + loading/error/hasMore） |
| store 沦为只读缓存 | `filesStore.uploadFile` 全仓 0 调用；上传/批量删除/删目录直连 service 再各自 `refresh()` | 写路径收口到 store action，或明确「只读」并删死代码 |
| loading 语义混用 | 同一 `loading` 既表示首次加载又表示「正在改」→ 删一个文件整屏闪骨架、滚动位置丢失 | 拆 `listLoading` / `loadingMore` / `mutationPending` |
| 目录浏览 4 套 | `DirectoryTree`、`BreadcrumbTreeMenu`、`MoveDialog`、`AccessPasswords` 各写一份加载/缓存/排序 | `useDirectoryListing` + 共享 `Map<path, entries>` |
| 搜索 3 套状态机 | `useFileSearch`、`UnifiedSearch`、`BrowserSearchBox`+`MobileSearchBar`，外加侧栏死控件 | 搜索状态进 store，三处 UI 共用 |
| 双渲染 6 处 | 文件列表、FileItem、AdminUsers、AccessPasswords、AccessTokens、SharedLinks 都是「表格 + 卡片」两份，按钮与 aria 写两遍 | 单份行组件 + CSS 切换 |
| 模板 ref 反向操纵 | Home 持 12 方法句柄，移动底栏靠 ref 转发；FileBrowser 自己也渲染同一组按钮 | 选择/批量提升为 selection controller（provide/inject） |
| 错误处理三套 | 32 处 `catch {}` 静默（树加载失败当「无子目录」、会话列表失败当「无会话」）、toast、内联并存 | 约定三类：主数据内联+重试 / 附属数据横幅 / mutation toast |

## 四、P1 — 样式系统

- **全局文件互相覆盖**：`controls.scss:268-273` 与 `modern.scss:36-43,207-214` 对同一控件分别写 8px / 3px，导入顺序决定胜负（controls 的 8px 是死声明）；焦点语言也有 `box-shadow` 与 `outline` 两套。
- **`modern.scss` 内部重复**：`.button`、`.input`、`.table` 各写两遍（6-149 与 163-296），`.dropdown-content`/`.dropdown-item` 与 controls 三处交叠。
- **表头 6 份手抄**：9 张表只有 3 张挂 `.table`，其余各抄 11 行并已漂移（`13px` vs `0.8125rem`、个别缺 `vertical-align: middle` 与 `line-height`）。
- **卡片头 5 种间距**、**空态 5 套**、**徽标 5 套**、**spin 动画 9 份**、**reduced-motion 样板 17 份（48 处 `!important`）**。
- **令牌层**：深色缺 5 个圆角令牌（靠 `:root` 兜底）、布局块 34 行复制两份、9 个 0 引用令牌、`--vf-radius-card`=`--vf-radius-sm` 等同义重复、`--vf-row-height-compact: 44px` 与实现 38px 冲突。
- **影子令牌**：40 个组件内自定义属性（`--auth-*` 19 个、`--vf-header-*` 5、`--vf-sidebar-item-*` 6…）只是别名；`--sf-topbar-height` 是 Seafile 遗留命名。
- **对比度**：`--vf-text-subtle` 浅色 `#8a94a2` 对白底 **3.07:1**，却用于文件大小/时间等实义文字（108 处）；应收到 placeholder/disabled 或加深到 ≥4.5。
- **断点**：768/1023/1024/600/480 + 11 个一次性值；`AdminLayout` 的 `max-width:1024` 与 `Home` 的 `min-width:1024` 在正好 1024px 时同时成立。
- **检查器盲区**：`is-*` 前缀全豁免（77 个类免检，`.is-xs/.is-lg` 已是死变体）、`tests/` 被算作引用（`browser-sidebar` 靠「断言不存在」的测试保活）、硬编码色只查 `#rrggbb`（`hsl()` 漏网）。

## 五、P1 — 性能与构建

- **首屏三次串行往返**：`fetchMe` → `session/bootstrap` → 才 `import(Home.vue)`，且 `index.html` 没有 Home chunk 的 `modulepreload`；期间 `#app` 空白无骨架。建议合并 bootstrap 与 me、预加载 Home、放 CSS 启动占位。
- **CSS 首屏 565 KB，其中约 183 KB 是同一套主题变量的三份副本**（`:root` 60 KB + `[data-theme=light]` 60 KB + `@media prefers-color-scheme:light` 57 KB）。让 `theme-init.js` 始终写 `data-theme` 即可删掉 media 那份。
- **体积预算口径虚高**：`check-size.mjs` 把 `.br/.gz` 副本也计入并再压缩一次，实测 gz 报 1030 KB 而真实只有 364 KB → 预算等于失效。过滤掉压缩副本并重设预算。
- **图标被切成 19 个 151–555 B 的 chunk**，每个一次请求；用 `manualChunks` 合并。
- **大列表无虚拟滚动**：首屏 40 条、每次 +30，滚到 1000 条后 DOM 常驻上千行；`FileItem` 收 20+ props/18 事件，选中一个文件会整列表重渲染；全仓无 `v-memo`/`shallowRef`。
- **computed 全量排序被重复衍生**：`sortBrowserItems` 还多一次 `[...items]`；每页追加都整体重排；列表统计 4 个 computed 各遍历一遍。
- **每次切目录重取全量分享集合**（`getSharedPaths`），大目录下固定多一次 RTT。
- **死代码 1091 行**：`SidebarOverview.vue`(609)、`DesktopCommandBar.vue`(406)、`IconButton.vue`、`ErrorMessage.vue` 在 `src` 内 0 引用，仅靠测试保活。

## 六、已核查无问题（避免误判）

`@tabler/icons-vue` 按需导入正确；16/16 路由懒加载；表格 `th scope` 全覆盖；无正数 `tabindex`、无键盘陷阱；状态不只靠颜色；`:focus-visible` 与 reduced-motion 覆盖到位；破坏性操作均有确认；上传/下载可取消；`v-for :key` 稳定；`window/document` 监听基本成对；图片懒加载已用于网格与详情。

## 七、建议整改顺序

1. **半天**：P0 的 1/2/3（批量条、缩略图、选择泄漏）+ 删死代码 + 预算脚本口径修正。
2. **一天**：P0 的 4–10（可访问性与移动端安全区/z-index）+ Modal 语义与 Esc 栈（一次改完 5 个问题）。
3. **一个小迭代**：`useListResource` 收口 9 个 reload、loading 拆分、store 写路径、错误处理约定。
4. **排期主题**：文件列表重构（拆 FileBrowser、合并双渲染、虚拟滚动、逻辑出组件）+ 样式收敛（删除重复全局块、表头/卡片头/空态/徽标统一、令牌瘦身）+ 开 `strictTemplates`。

> 行号均由 `grep -n` / `sed -n` 逐一复核；对比度、chunk 体积、CSS 字节数为 Playwright 与 node 实测。
