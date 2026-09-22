# 设计令牌与刻度（DESIGN TOKENS）

## 0. 完备性一览（round 84 总评 ✓ 全轴有数据背书）

| 轴 | 状态 | 定档轮 |
| --- | --- | --- |
| 颜色/对比度 | ✓ 全 AA + 目检 | r7/17/36 |
| 形状（圆角） | ✓ 硬编码归零 | r45 |
| 间距 | ✓ 节奏基线 + 存量决策 | r45 |
| 排印 | ✓ 15 档刻度记档 | r47 |
| 图标 | ✓ 尺寸阶梯 + stroke 反向 | r48 |
| **阴影/elevation** | ✓ token 22/24 + 死 token 归零 | **r84** |
| 状态层（hover/press/focus） | ✓ 13/13 终审 | r40/46/49 |
| 动效（M3） | ✓ 曲线统一 + reduced-motion 二部 | r39/53/54 |
| 语言（文案） | ✓ 四维 + 语气分层 | r73 |
| 图形（空态插图） | ✓ 双档阶梯判档 | r83 |
| 组件参数（family） | ✓ 速查表 | r5/49 |
| 可访问性 | ✓ 键盘×屏读双线 | r62-69 |

> 48 轮迭代校准的单页参考（round 49 汇总）。新样式取值先查此页；
> 令牌定义在 `client/src/styles/theme.scss`，全局控件语言在 `client/src/styles/controls.scss`。

## 1. 颜色（双主题语义令牌，`--vf-*`）

| 令牌 | 浅色 | 语义 |
| --- | --- | --- |
| `--vf-accent` | `#2563eb` | 主色（与 Bulma `--bulma-primary` 对齐） |
| `--vf-accent-strong` / `-text` | `#1d4ed8` / `#1e40af` | hover 强调 / 蓝字（AA 7.08） |
| `--vf-accent-soft` / `-soft-strong` | `rgba(37,99,235,.08/.16)` | 状态层：悬停 8% / 选中·按压 16% |
| `--vf-focus-ring` | `rgba(37,99,235,.3)` | 输入类聚焦环 |
| `--vf-surface` / `-raised` / `-sunken` | `#fff` / `#fff` / `#f5f8fc` | 面板三级（深色为 tonal 阶梯） |
| `--vf-chart-*` | 文档蓝/图片绿/视频紫/音频琥珀/其它灰 | 存储条分类色板 |

文字四档：`--vf-text` 主体（7.4:1+）/ `--vf-text-strong` 强调（15:1）/
`--vf-text-muted` 弱化（4.6:1 AA）/ `--vf-text-subtle` 更弱（辅助文字）。
语义通道（danger/success/warning/info × h/s/l + soft/text/line/invert）经 AA 校准（round 7/17）。

## 2. 形状（圆角四档 + 语义值）

| 值 | 令牌 | 用途 |
| --- | --- | --- |
| 4px | `--vf-radius-xs` | 内联标记（diff 词段、小 chip） |
| 8px | `--vf-radius-sm` | 控件/条目/浮层（主档） |
| 10px | `--vf-radius` | 中卡/输入框 |
| 16px | `--vf-radius-lg` | 弹窗/大卡 |
| 9999px | `--vf-radius-pill` | 药丸（搜索框/徽章） |
| `50%` / `0` | — | 圆形插图 / 满铺行染色（用户定稿方角） |

**硬编码圆角 = 0**（round 45 归一 ✓ 109 处全 token/语义值）。

## 3. 间距

- **节奏基线（新样式取值）**：`0.25 / 0.5 / 0.75 / 1 / 1.5 / 2rem`（4px 节奏）；
- 存量：0.05–0.9rem 14 档细粒度 = 历轮校准值，**不强改**（round 45 决策）。

## 4. 排印

- 字号刻度：`0.7 / 0.72 / 0.74 / **0.75(12px 锚)** / 0.76 / **0.78(主档)** / 0.8 /
  0.82 / 0.84 / 0.86 / **0.875(14px 锚)** / 0.9 / 0.95 / 1.05`；
- 字重：400 正文 / 500 次强调 / **600 强调主档** / 700 标题；
- 行高：1 紧凑 / 1.3–1.4 正文紧 / **1.5 主档** / 1.6 宽松。

## 5. 图标

- 尺寸：`12 / 14 / 15 / 16(主) / 18 / 20 / 22(触屏) / 24(投放) / 32 / 40 / 42(插图)`；
- **stroke 随尺寸反向**：42→1.3、40→1.4、32→1.5、行内 18→1.7（视觉重量补偿）。

## 6. 状态层与反馈（双主题 AA 全达标）

| 状态 | 视觉 | 备注 |
| --- | --- | --- |
| 悬停 | accent-soft 8%（0.04–0.06 局部档） | forcePseudoState 实测（r49） |
| 选中 / 按压 | accent-soft-strong 16% | 按压 = 高悬停一档（r40） |
| 聚焦 | 2px solid accent 环 / 输入 shadow 环 / 原生 auto | 13/13 终审（r46） |
| 拖放落点 | accent-soft + 1px 虚线 accent | 列表/树同语言 |
| 行染色 | **满铺无圆角** | 用户定稿（§4.45） |

## 7. 动效（M3）

- `--vf-motion-standard: cubic-bezier(0.2, 0, 0, 1)`（状态过渡/入场，M3 Standard==Emphasized）；
- `--vf-motion-exit: cubic-bezier(0.3, 0, 1, 1)`（Accelerate 离场）；
- **loading/shimmer 一律 `linear`**（M3 规范）；时长惯例：120ms 微反馈 / 150ms 状态 / 200ms+ 结构。

## 8. 组件语言速查（family 参数，round 5 起校准）

| 组件族 | 参数 |
| --- | --- |
| 控件（按钮/分段） | 36px 高药丸（`vf-*` 三档按钮 × ghost/primary/icon） |
| 主行动按钮 | 40px 高（搜索/上传 CTA） |
| chips / 徽章 | 32px 高（M3）；状态徽章小号 pill |
| 弹出浮层 | surface-raised + border-weak + 8px + shadow-menu；玻璃态须 `blur(8px)` + 半透明令牌 |
| 卡片 | 14px 圆角 + border-weak + shadow-card；内边距 1.1/1.2/1.3rem 三档 |
| 列表行 | 桌面 48px / 移动 87px（骨架同高 ✓ r43/44 实测） |
| 网格卡 | `--file-card-min = round(缩略图 × 1.35)`（144 → 194px）+ gap 14px |
| 页面框架 | `.vf-page-card / .vf-page-title / .vf-page-subtitle` + `.vf-status-pill` |

## 9. 响应式断点（4 档定档，round 91 归一）

| 断点 | 语义 | 锚 |
| --- | --- | --- |
| `max-width: 480px` | 手机窄屏排版 | 常见手机档 |
| `max-width: 768px` | 平板/窄栏折叠 | **Bulma tablet 锚** |
| `min-width: 900px`（配对 899） | **历史分栏专属**（有意档 ✓） | 分栏可读宽 |
| `min-width: 1024px`（配对 1023） | 桌面 | **Bulma desktop 锚** |

语义查询（非断点 ✓ 照用）：`prefers-reduced-motion` / `hover: none` /
`pointer: coarse`。**新增断点先查此表**（r91 前 8 值散乱 → 归一 4 档 ✓ 760/700/520
三散值并档）。

## 10. 语言风格（文案四维，round 73 评审定档）

1. **标点**：句末**不加句号**（中文 UI 惯例 ✓）；补充从句用逗号；
   行内引用用「」（如「预览」或「对比」）。
2. **称呼**：**零称呼**客观陈述（不用 你/您 ✓ 全仓一致）。
3. **语气分层**（有意设计，非分叉）：
   | 式 | 用于 | 例 |
   | --- | --- | --- |
   | **「还没有 X」** | 可创建项（期待性，配行动钮） | 还没有访问令牌 / 还没有分享链接 |
   | **「暂无 X」** | 只读累积项（中性） | 暂无历史记录 / 暂无审计记录 / 暂无用户 |
   | **「名词 + 失败」** | 错误态 | 加载失败 / 预览失败 / 加载历史记录失败 |
   | **「动作 + 结果」** | 成功/回执 | 目录创建成功 / 已恢复并生成新版本 / 名称未变化 |
4. **提示句（hint）**：以动作或解释收尾（「…上传」「…查看」「…显示在这里」✓）；
   空态三件套 = 标题（式 3）+ 提示 + 行动钮（可选）✓ 结构统一。

## 10. 约定

1. `--vf-*` 字面值**只出现在声明处**，组件一律引用变量；
2. 新增样式取值先查本页刻度（离散档优先于随手值）；
3. 交互件尺寸语言：控件 36px 药丸 / 主行动 40px / chips 32px（M3）；
4. 提交前跑**五门禁**：`bunx vue-tsc --noEmit` → `bun run check` → `bun run build`；
5. 状态审计工具语：`DOM.querySelectorAll` + `CSS.forcePseudoState`（强制 hover/press/focus）；
   条件渲染目标须 `offsetParent !== null` 过滤 + 目标专属文本锚。
6. **reduced-motion 约定**（r53/54 二部全绿）：循环动画（spin/shimmer/pulse）与
   **动效类过渡**（transform/width/all）必须有降级块（`animation-duration: 0.01ms +
   iteration 1 + transition-duration: 0.01ms`）；色/透明/阴影淡入**豁免**（无位移风险）。
