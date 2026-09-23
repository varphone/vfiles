# 探针与流程工具语（PROBE IDIOMS）

> 82 轮实测事故沉淀的可查表（每条附来源轮次）。「先查后写、判据可见、证据确定」
> 三原则的具体化。探针受阻时先翻此表。

## A. 夹具与环境

| # | 工具语 | 来源 |
| --- | --- | --- |
| 1 | fixture CLI/服务器**同 shell 导出 `VFILES_*`**（裸跑会写进仓库 `data/` ✗ 事故级） | r23 |
| 2 | 服务器/夹具/探针**单命令起停**（跨工具调用 `/tmp` 会被宿主清理 ✗ 多次） | r31/r38 |
| 3 | 二进制夹具用 **node 写盘 + 字节数校验**（shell 传 base64 会损 ✗ 坏图假阴性） | r71 |
| 4 | CJK 测试内容**避开撞名**（唯一命名 / 成对 from→to 路径跟进） | r35 |
| 5 | `keyboard.press('字符')` 无效（要键名）；中文键入用 `type()` 或**下条** | r76 |

## B. 浏览器探针

| # | 工具语 | 来源 |
| --- | --- | --- |
| 6 | **CJK `keyboard.type()` 走 `insertText`、不产生 keydown** ✗✗ → 处理器永不会触发 → 用 `document.dispatchEvent(new KeyboardEvent('keydown', { key: '字' }))` | r78（破案钥匙） |
| 7 | **可见实例判据**：Modal **藏而不卸** → 裸 `querySelector` 命中隐藏实例 → **`offsetParent !== null` 过滤 / `.modal.is-active` 作用域** | r43 起 N 次 |
| 8 | **规则挂载层先查**（`is-dragging` 挂 `> td` 非 tr；hover 挂 `.directory-tree-row` 非 item）→ 探针目标对层 | r49/r56 |
| 9 | **类名先查实文**；疑回归先 `git log -S` 考古再定性（drag-chip"丢失"虚惊） | r56 |
| 10 | `grep` 歧义：`key`/`title` 等通用变量名撞语义 → **上下文核查**（`(匿名)` 筛选键虚惊） | r81 |
| 11 | **window ≠ document 挂点**（grep `addEventListener`/`@keydown` 对准再发事件） | r77 |
| 12 | 事件目标类名/dump：**输出原始 className 不猜判据**（dump 式证据） | r56 |
| 13 | 判据设计走**中间态/非默认**（末页 PageDown 无信息盲点 ✗；选中行无悬停增量 ✓ 语义对） | r74/r40 |
| 14 | `head -N` 截断会吞目标行（分批/放宽 ✗ 多次） | r81 等 |

## C. jsdom / 单测

| # | 工具语 | 来源 |
| --- | --- | --- |
| 15 | jsdom 无 `DragEvent` 构造 → `MouseEvent('dragover', {clientX…})` 冒充 | r57 |
| 16 | jsdom 无 `elementFromPoint` → 桩 `document.elementFromPoint = () => el` | r58 |
| 17 | **waitFor 等内容而非元素存在**（首渲染暂存值假阴 ✗） | r65 |
| 18 | `flush: post` 监听器晚于同步断言 → **断言候 tick** | r65/74 |
| 19 | 回常态桩**不能桩回拖拽行自身**（= 仍禁止态 ✗）→ 桩 `document.body` | r59 |
| 20 | 单测失败先**隔离跑**（判污染 vs 真破 ✓ r65/75 皆靠它定性） | r65 |

## D. 编辑与流程

| # | 工具语 | 来源 |
| --- | --- | --- |
| 21 | **改 .vue 前 sed 打印 prettier 格式化后实文**构造锚点（折行失配 ×N ✗ 原子断言防半态 ✓） | r38 起 |
| 22 | 多站点同文案用**行号锚 + 自底向上插入**（6 处同绑定盲换会污染他组件） | r61 |
| 23 | 模板改造收口错位 → **区段开合行号清单**（awk 列 tag）定位补平 | r42 |
| 24 | **`ref` 声明 ≠ 绑定**：grep `ref="名"` 确认模板绑定（永 null = 死代码 ✗ r27 至 r71） | r71 |
| 25 | **prettier 只跑改动文件**（整目录跑 = 无关文件误格式化 ×2 ✗） | r61/65 |
| 26 | `withDefaults` 接口式 props：options 式默认值静默成"默认值对象"（tsc TS2339 揪出） | r61 |
| 27 | **五门禁缺一即漏**：`vue-tsc` + `check`（lint/lint:styles/test）+ `build`（vitest 不做类型检查 ✓ check 不含构建 ✓） | r43 |
| 28 | **表漂移守护式断言**（键位改动忘登记 → 测试揭发 ✓ 文档-代码同步新形态） | r81 |
| 29 | 疑似功能丢失先查**设计语义分层**（「还没有」vs「暂无」非分叉 ✓ 别急着"统一"） | r73 |
| 30 | 行为码健在 ≠ 有保障：**零测试即缺口**（HUD 勘误轮教训 ✓ 补测优先于补功能） | r77 |
| 31 | **命令链一律 `&&`**（断言败即停 ✗ 换行分隔让 git 在断言失败后照跑 = message 声称未落盘内容） | r82 自纠 |
| 32 | **绝对路径前缀**（r105 强化 ✗✗✗ 三录：跨调用 cwd 不延续（新 shell）——连相对都别想，一律绝对路径） | r83/105 |
| 33 | **replace 编辑一律附 assert**（不验证 = 静默失败；r82/r87 两轮 message-vs-disk 漂移之源） | r87/88 勘误 |
| 34 | **同文多匹配** → 专用 class（`.auth-submit`）或 `getByRole` 稳式（role 互斥即零撞名） | r88 |
| 35 | **模式切换 × 提交链 = jsdom 脆点**（3 轮 3 种失败 ✗ 收窄记档、留浏览器探针线） | r88 |
| 36 | **移动行操作菜单定位链 = 浏览器探针脆点**（r94×2 + r95 三败止损 ✗ 移动 modal 采样改借 ui:sweep 移动帧顺访） | r95 |
| 37 | **字面批改对转义正则漏网**（`2\\.0` 正则内嵌逃过 `\d+\.0` 批改 ✗✓ 改后须 grep `\\.0` 类残留） | r96 |
| 38 | **同名本地函数 = 替换陷阱**（改体成自调 = 无限递归 ✗ 归一前先 grep 同名声明） | r96 |
| 39 | **FAIL 名连文件读**（it 名 grep 落错文件 ✗✗ `FAIL tests/X > it` 全行读）；**assert 表达期望勿妄值**（计数臆断会中止正确编辑） | r97 |
| 40 | **脚本改动 = `node --check` 速验**（五门禁不含 .mjs ✗✓ r99 首验后置实证；当轮即立） | r99/101 |
| 41 | **字符串采样勿截断入档**（dump `.slice(0,4)` = "新建文件"假值 ✗✗ 真值「新建文件夹」= 判据 #14 新族；采样全值、截断仅展示） | r101 |
| 42 | **写文件前 `mkdir -p`**（目录缺则 cat 静默败 ✗ echo 换行假绿 = #31 复犯实录） | r102 |
| 43 | **grep 全树必限 src**（`target/` 编译产物 = 脏源（impl BackendDeps 落到二进制 ✗）；定点 sed 优于全树 grep） | r102 |
| 44 | **grep 模式 `-` 开头 = 被当 flag**（`-->` 报「未识别的选项」✗✓ 用 `-e` 或前缀通配）；**判词链反写自查**（二分对而 echo 反 ✗） | r104 |
| 45 | **grep 前证文件存在**（多文件 grep 对缺失文件静默 ✗✗「零命中」≠「无此功能」——admin.service.ts 缺失误判为零命中） | r107' |
| 46 | **借用不跨 await = 编码模板纪律**（r105/108'/109a **三号实录** ✗✗✗ async handler 参数一律纯拥有（调用侧同步提取）——写 handler 前自查模板） | r105→109a |
| 47 | **判据 grep 词边界**（`error` 撞 crate 名 "quick-error" ✗ `^error`/`\\berror\\b` 式 ✓ 判词链反写二号） | r106 |
| 48 | **管道掩错 = 门禁假绿险**（`tsc \\| head` = head exit 0 ✗✗✗ 历轮静默过含险 ✓ **`set -o pipefail` 强制** + TSC_EXIT 真绿验记式） | r106 |
| 49 | **grep 锚形盲区**（简写字段 `ftp,` 无冒号 ✗✗ `grep ftp:` 假"零命中"烧三轮 ✗✓ 锚须含简写形/错误区**无窗直读**（#14 三犯连环截教训） | r109b |
| 50 | **二分暂换当轮必还原**（`.fallback(hello)` 未换回 = **五轮 dispatch 全死码** ✗✗✗ 史诗盲区 ✓ 还原须 grep 验证；**dbg-eprintln = 断言级 debug 够不着时的一击破案式** | r105→110 |
| 51 | **`cmd \| tail` = SIGPIPE 假败**（BUILD_EXIT=101 非编译错 ✗ 免管道 `\>\/dev\/null && echo OK` 直验式） | r110'a |
| 52 | **Rust 改动 = `cargo build` 前置**（check ≠ build ✗✗ target/debug 旧二进制 = 探针旧产物病型三犯（前端 build/Rust build 同族））；**spawn 模板 = `tokio::select!`**（shutdown 等待在 run 前 = run 永不执行 ✗ 骨架 bug 警） | r110'a |
| 53 | **输出语义分层**（用户报 ✗ 调试注记/符号标记经 print 泄漏到命令输出 = 看似报错日志 ✗ 命令输出只载事实；注记归 commit/docs；回复用纯中文） | r113' |
| 54 | **fireEvent 修饰键回显存疑**（jsdom `fireEvent.keyDown({ctrlKey:true})` 实际未置修饰位 ✗ 自构造 `new KeyboardEvent` + dispatch = 稳式（r77 同族）） | r124 |
| 55 | **playwright `check()` 对 Vue 受控 checkbox 不触发 change 链**（五轮误报史诗 ✗✗✗ 稳式 = `input.click()` 单发（自带 change ✓ click+dispatch 双发 = 奇偶抵消）；**混类/工具-框架交互**为探针两大深水区） | r137→144 |
| 56 | **pkill/pgrep -f 自匹配自杀**（模式串含目标名 = 杀自己 shell ✗✗ 用 `-x vfiles` 精确名/倒排法） | r204 |
| 57 | **curl ASCII 全绿 ≠ 真实客户端可用**（href 合法性 + percent 语义只有真客户端暴露 ✗✗ 双斜杠/未解码 = 真实客户端丢条目或 404）+ **secret 长度<32 fixture 静默断链二犯**（≥34 保） | r204 |
| 59 | **单层延迟测量误导归因**（HTTP 总时 = 认证 hash + 查询 + 流转 ✗✗ r3 把 6ms/文件归 open ✗ 实则 hash 主导；**分层法定案** = SQL 直测（sqlite timer）+ 总时差值；同族 = **门禁与服务同 job 的探针早发**（0.0001s = refused ≠ 代码错 → 等端口循环）| r4 |
| 58 | **残留进程跑 deleted 二进制**（`/proc/pid/exe -> (deleted)` = 旧码响应 ✗✗ 排障先核监听进程 exe 时间 vs 工作区二进制；**bind 静默类** = spawn Ok ≠ bind 成功，权威行须在 bind 后打） | r205 || 58 | **残留进程跑 deleted 二进制**（`/proc/pid/exe -> (deleted)` = 旧码响应 ✗✗ 排障先核监听进程 exe 时间 vs 工作区二进制；**bind 静默类** = spawn Ok ≠ bind 成功，权威行须在 bind 后打） | r205 |
