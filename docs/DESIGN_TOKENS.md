# 设计令牌与刻度（DESIGN TOKENS）

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

## 8. 约定

1. `--vf-*` 字面值**只出现在声明处**，组件一律引用变量；
2. 新增样式取值先查本页刻度（离散档优先于随手值）；
3. 交互件尺寸语言：控件 36px 药丸 / 主行动 40px / chips 32px（M3）；
4. 提交前跑**五门禁**：`bunx vue-tsc --noEmit` → `bun run check` → `bun run build`；
5. 状态审计工具语：`DOM.querySelectorAll` + `CSS.forcePseudoState`（强制 hover/press/focus）。
