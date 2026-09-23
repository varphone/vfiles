# rsync 协议 daemon（r2 选型 → … → r8 对答表 → **r9 协议 30 实装 + 真 CLI RC0 列文件** ✗ delta r10）

## 状态（r9 末 · 权威源驱动实装 = 猜测层清零 ✗ 真 `rsync --list-only` RC0 兑现）

- **实装兑现（终极探针 ✔）**：真 `rsync 3.2.7` 对 `rsync://127.0.0.1:8874/files/` 全链通过：
  - 列模块 `rsync rsync://host:port/` → `files` RC0
  - `--list-only …/files/` → 4 行 RC0（含中文名 `说明.md`）
  - `-r --list-only …/files/aka/` → 9 行嵌套树 RC0（`foo/bar/hallo/*.tar.xz` 全相对路径名）
  - 未知模块 → `@ERROR: Unknown module 'nope'` + code5 = **逐字同官方**
  - 下载 `rsync -r …/hallo/ /tmp/dl/` → **code12**（delta 未实现 = r10 首件）
- **五处猜测校正**（`git clone` 官方源后逐条对源码）：
  1. mux 头 `[len_lo,len_mid,len_hi,7]`，**len = payload 长**（≤16MB，非 255 ✗ 旧 assert 债清）
  2. `81 FE` = `compat_flags` **varint**（510=0x1FE）；客户端 `-e` 串含 `v` → `CF_VARINT_FLIST_FLAGS`
     → flist xflags 必须走 **varint**（0x9A→`80 9A`、0xFE→`80 FE`）
  3. 校验和协商 = 双向 **vstring**（服务端 `#`=len 35 + `"xxh128 … sha1 none"`，无换行）
  4. `46 19 14 67` = **`checksum_seed`**（`time^pid`）非字面常量 → 随机生成
  5. 收尾 = 收 3×NDX_DONE → 发 2×NDX_DONE + 终结 NDX_DONE → 5×varlong30(3) 统计 → 末 NDX_DONE
- **实装结构**（`crates/vfiles-rsync/src/lib.rs` 全重写，1012 行）：
  - wire 原语 `write_varint`/`write_varlong`（含 >8MB extra 域）/`write_vstring`/mux 帧 = io.c 移植
  - `encode_flist`：xflags（`SAME_UID|SAME_GID` 恒置 + `.` 置 `TOP_DIR` + `SAME_MODE/TIME` 差分、
    **不置 NO_CONTENT_DIR** = 真机两例实证）→ l2+全名 → length(varlong3) → [mtime varlong4] →
    [mode LE4]；收尾 `00 00`
  - L1 `handle_conn`：banner → 模块列表/选择/P04 → args（NUL 双尾 + `-e` 串提取）→
    compat_flags → 协商 vstring → seed → filter list（mux 解复用）→ flist → NDX 收尾；
    数据源 = **闭包注入**（duplex 单测零 domain 依赖）
  - L2 `collect_flat`：默认 ns 深度优先前序 + 子路径前缀剥离；**不置 CF_INC_RECURSE** = 单 flist
    全量（规避增量递归面，客户端读到 `00 00` 即止 ✓ 真机已验证）
  - bin 装配：`build_and_spawn_rsync` 传 `entry_repo` + 默认 ns（与 WebDAV/S3 同源）
- **门禁**：`cargo test -p vfiles-rsync` 10/10（真机转录逐字节 golden ×2 + 整链 duplex + varint/
  varlong/vstring 移植物）× `cargo test --workspace` 全绿 × clippy 零告警 × fmt 干净 × bin build 绿。
- **r10 清单**：sender **delta**（`send_files` 文件请求 → `sum_head` + token 流 literal/匹配 →
  真 CLI 文件落地字节比对）→ 收端 push（复用 upload 链）→ secrets 认证 → `--checksum`/压缩面。
  ✗ **整文件下载协议已真机转录入档** `golden/download_wire_r9.md`（请求帧 iflags 0xA000 + 全零
  sum_head、回显 + `int32(len)+data` token 流、校验和 = **MD5(内容) 无 seed**、ndx = f_name_cmp
  排序下标）→ r10 零猜起步。

## 状态（r8 末 · 双向对答表提取 + preserve/mode 语义解 = 实装钥匙全齐零猜）

- **对答表资产**（`golden/wire_dialogue_r8.md` ✗ strace 双向流按 fd3 分向 = 收流/发流两序
  直接照推状态机）：收 args 双 NUL → 发 `81 FE` → 收 `1E`+30B checksum 串 → 发 `#…none\n`(36B
  字面常量)+`46 19 14 67` → 收 `04帧` → **发按树编码的 57B flist** → 尾三帧配对（收 01/03/01 →
  发 01/02/10·20B 原样记档可疑摘要帧）✗ 顶部 8×832B ELF = 启动噪声剔除 ✓。
- **差分位方案定稿** ✨：`SAME_UID|SAME_GID 恒置`（`--list-only` 接收端 !preserve = 转录首条
  `0x19`=TOP|UID|GID 全通 ✗ 后续 `0x18`）+ SAME_MODE/TIME 按与前条差分写值 + 不用 SAME_NAME/
  LONG_NAME（恒发全名 ≤255）✗ **mode = 原生 stat mode u32 LE 恒等实证**（golden 两独立命中
  = `to_wire_mode` 无需源 ✗ 本轮 clone 120s 超时复发 = 网络不稳但已无关）✗ mtime=varlong(min4)
  紧凑（created_at unix 秒可接受）· length=varlong30(min3)（dir 4096 / file meta size、fallback
  0 记档）· name = l2 byte + basename 递归树序。
- **r9 终清单**（实装+探针 = goal 收官路径）：ListCtx 依赖入 crate（vfiles-app/domain path
  house 式）→ OK 后状态机五步 → flist 编码器 → duplex 字节断言单测 → #61 三跑 → bin 传 ctx
  → **终极探针 = 真 `rsync --list-only rsync://…/files/` = RC0 列 vfiles 文件**（CLI 报错循环
  修）→ delta（r10）。

## 状态（r7 末 · git clone 权威源 + **mtime 解出** = flist 编码法全知 = 实装无阻）

- **通道胜利**：`git clone --depth 1` 官方 rsync（本机 ✗ 避开 r6 web_fetch 30s/反爬双败）
  → flist.c 4003 行本地权威 ✗ **完整编码法入档 `golden/frame_codec_r7.md`**：
  XMIT 位表（rsync.h 18 bit）· 字段写序（send/recv 双向源）· **varlong(min) 紧凑形**（ctrl
  在首判 extra 表、值 LE 尾接、ub 整读 LE ✗ varint 同构 · int=平台 LE ✗ shortint=BE 混用
  记档）· varlong30≥30 直通（我们锁 30 ✓）。
- **mtime 解出 ✅✅**：`6a 51 86 b3` = varlong(min4) → ctrl=0x6a(extra0)+ub LE = **0x6AB38651
  = 1790150225 = epoch 完全命中**（r6"交错"谜团 = **ctrl 首 + 值尾**布局）+ 首条 `.` 全链
  解通：xflags=0x19=TOP|SAME_UID|SAME_GID 语义吻、length varlong30(min3)=**4096** ✓、
  name l2 byte ✓ —— **57B 逐字段闭环、r8 编码器可逐字节复刻**。
- **r8 实装清单（无阻）**：args 解析 → `\x81\xFE` → `\x1E`+checksum → `#选定\n`+4B → mux 收
  → **flist 帧编码（差分 SAME_* 位 + mode=to_wire_mode 源同步）** → 终结帧配对表对答 →
  树→条目流 → **终极探针 = 真 `rsync --list-only rsync://…/files/` = RC0 列 vfiles 文件**
  → delta（r9）。块头语义（`35…`/`81 FE 46 19 14 67` 连读）= 实装时与 strace 前段一并定
  ✗ 不阻条目层。

## 状态（r6 末 · P04 真 CLI 完胜 + 57B 帧三字段反推解 = mtime 权威通道待 r7）

- **P04 三重兑现终验（r4 承诺 + r5 修 + r6 CLI）** ✨：`rsync --list-only …/nope/` 输出 =
  `@ERROR: Unknown module 'nope'` + 客户端 code5 行 =**与官方 r5 记录逐字同**（我方从三行
  （多一句 didn't-get）→ 两行 = 形差清零 ✓）+ P01 = RC0（`files          \t` 收尾金标准保持
  ✓）✗ 门禁：rsync 3 测 0.00s、BUILD=0、前置逐断言 #60 ✓。
- **57B flist 帧三字段反推解**（资产 = `golden/frame_layout_r6.md`）：**name = 1B 长度+bytes
  +\0（三锚 01/03/05 独立证实）· mode = u32 LE（双命中与 ls 值一致）· size = u16（双命中）**
  ✓✓ 头部 `35 00 00 07 A0 19` = mux/flist 头层。
- **mtime 未解（差一层）**：epoch u32/u64/±1天/float 全扫零命中 ✗ 帧中 `6a 51 86 b3` 与
  epoch 字节 `6a b3 86 51` **中间两对交错**（疑 16 位半段重排/varint 变形）✗ **双网络源失**：
  GitHub raw 30s 超时、manpage 反爬壳（312 词无正文）✗ → **r7 权威通道 = GitHub blob 行号页**
  （flist.c L1841/L3204 send/read_file_entry 段）。
- **r7 清单**：mtime 权威解 → 五层实装（args 解析 / `\x1E`+checksum `#` 回 / mux+flist 编码 /
  终结帧对答 / 树→条目流）→ 终极探针 `rsync --list-only rsync://…/files/` = **RC0 列 vfiles
  文件** → 然后 delta（r8）。

## 状态（r5 末 · strace 直挂 = 双端全字节 wire 转录到手 = 实装钥匙全齐）

- **P04 官方形修兑现（r4 承诺）**：未知模块回 `@ERROR: Unknown module '<name>'` 单行即关
  （与官方逐字节同形 ✗ 断言同步 ✗ 三黄金 0.00s 绿 ✗ 真 CLI 待重启复验 r6 开工一并）。
- **wire 五层时序全解剖**（资产 = `golden/wire_anatomy_r5.md` 解剖图 + `golden/
  client_strace_raw.txt` 34KB 原始双端转录 sha=968dc6cd… ✗ **#66 strace 直挂 -f 立法**） ✨：
  1. 明文握手（banner 41B 版本+算法 → `GOLD\n` → `@RSYNCD: OK\n`）= **我们已同形 ✓**
  2. **命令段真形** = `--server\0--sender\0-de.LsfxCIvu\0--list-only\0.\0GOLD/\0\0`（真 flag 簇
     + 源 `.` + 模块内路径 + 双 NUL ✗ r4 python 模拟缺 flag 簇 = 对照差记档）
  3. 协议确认层 = 服 `\201\376`(2B) → 客 `\x1E`(=30 单字节) + checksum 串 30B → 服
     **`#` + 选定算法串 + `\n`**(35B = `#` 前缀 = 算法选定响应) + 4B `F\x19\x14g`
  4. **flist 57B 铁证帧**：name = **1B 长度 + bytes 双锚**（`03 sub\0` / `05 a.txt\0` ✓
     mtime/mode 段 = r6 按 rsync 源 flist.c 对齐解出）
  5. 终结帧族（`01\0\0\07\0` / `04\0\0\07 FF×4` / `08\0\0\21X\0…` 双 fd mux 对话 = 收尾层 ✗
     r6 精解对答）。
- **r6 实装清单**（五层照转录实现 + 终极探针 = 真 `rsync --list-only rsync://…/files/` =
  **RC0 列出 vfiles 文件**）：① args 解析（NUL 双尾 + flag 簇识别 `--list-only`/`--sender`）
  ② `\x1E` + checksum 串回 `#…\n` ③ mux 帧写 + flist 编码（1B len+name、mtime/mode 源对齐）
  ④ 终结帧对答 ⑤ vfiles 树 → 条目流（`collect_keys` 同族复用 ✗ mode 按 drwxr-xr-x/-rw-r--r-- 形）。
- 纪律：#62 串行 / #63 硬超时 / #65 五钟 / **#66 strace 直挂 -f（timeout 包装全空一次的教训）**。

## 状态（r4 末 · 官方 daemon 黄金对照法全胜）

- **收尾层完成（r4 主目标 = 金标准达标）** ✨：`rsync --list-only rsync://…/` **RC0**
  干净退出、输出 `files          \t`（**15 列填充 + tab** 与官方 `GOLD           \t`
  逐字节同构）✗ banner = `@RSYNCD: 30.0 sha512 sha256 sha1 md5 md4`（**版本+算法串**
  形补齐）✗ 收尾 = `@RSYNCD: EXIT\n` + 关连接（**破的百年疑案 = NUL 是 mux 帧的
  channel 字节、不是列表终结** ✗ r3 把它当 terminator = RC5 真身）。
- **对照法（r4 立法、后续全用）**：本机起**官方 `rsync --daemon`** 最小 conf → python
  扮客户端**同款三步双向 dump**（收 banner → 发 31 → 空行/选模块）→ 与自家逐字节列差异
  =**四层差一次全暴露**（banner 算法串 / 模块行 15+tab / EXIT 收尾 / 时机）。
- **Phase1 黄金资产已入库**（`crates/vfiles-rsync/golden/`）：选模块层 = `@RSYNCD: OK\n`
  （**我方已同形 ✓**）· 客户端 args = `--server\0--sender\0--list-only\0-v\0.\0\0`
  （NUL 分隔双 NUL 尾 = r5 解析形已知）· 官方首响应 = **5 字节 `06 fc 5d 11 67`**
  （mux 首帧待解剖 · rsync 源 mux 定义 + 更完整客户端模拟 = r5 两参照）。
- **r5 首件（一行修）**：P04 未知模块形对齐官方 = `@ERROR: Unknown module '<name>'`
  （单行即关 ✗ 现我方 `@RSYNCD: ERROR …` 多一句 didn't-get = 形差）→ 断言同步。
- 纪律践行全程：#62 串行 / #63 硬超时（每跑 timeout 60-300）/ #65 **五钟阈值**（本批
  一切跑秒级 = rsync 0.00s、full 26 套 9-10s、各 build/check 0-2s ✗ 无一次分钟级）。

## 选型定案：**自研 rsync 30 协议 + daemon 形**

| 判据 | 结论 |
| --- | --- |
| 现成 crate | **无成熟 Rust server**（HN 有纯 Rust protocol-32 重写在途 =参照对象，不可依赖 ✗） |
| 借道 C rsync --daemon | 不可行（存储在 sqlite/blobs ✗ 非裸 fs =无法旁路挂载）✗ |
| 协议 | **v30**（rsync 3.x 客户端全线兼容）：模块协商 + 文件列表 + 块校验 delta 下载/上传 |
| 形态 | **daemon 配置形**（`rsync://host/module` ✗ module = 默认 ns 顶层命名空间?r3 定：单 module `files` =默认 ns ✓） |
| 方向 r3 首版 | **只读拉**（list + download ✗ 备份场景 = rsync 从 vfiles 拉 ✓ 最常见）+ 推送 = r4（接收端写链复用 upload 链） |
| 认证 | 同 S3 env 单对 r2 先例（`VFILES_RSYNC_*`）或匿名只读（r3 按需定） |
| 端口 | **873 默认**（`VFILES_RSYNC_PORT`，env 关 =默认关显式启用同 S3 先例）✗ TCP 直协议 ≠ HTTP 栈 =独立 runtime 形（tokio::net::TcpListener 自处理 ✗ 无 axum） |
| 与内容哈希 | vfiles 已有版本链/内容寻址 = **delta 校验和天然契合**（rsync 强项 =增量 ✗ 我们的哈希是现成地基） |

## r3 工作分解（预告）

1. 协议编解码层（pkt 式长度前缀 + 模块协商 + 文件列表字节形）
2. 只读 daemon：`--list` + delta 下载（rsync 算法 = rolling checksum + 强校验和二层）
3. 集成探针：真 `rsync` CLI（全平台自带）`rsync -av rsync://host/module/ /tmp/` 一发实证
4. 记档扩展：checksum 模式（-c）、增量轮询（--checksum-choice）、xattr（可选）

## 不做/排除

- rsync over SSH 形（=SSH 面另算，不在 daemon 形内）
- DeltaV（WebDAV 版本扩展 =客户端生态已死 ✗ 见主目标评估表）
- NFS/SMB（战略级另立项，非近期路线）
