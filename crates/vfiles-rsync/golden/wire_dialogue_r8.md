# rsync-30 完整对答表 + 差分位方案（round 8 · strace 双向流提取 = 实装零猜终章）

提取法：解析 `client_strace_raw.txt`（八进制转义还原）按 fd3 分 write/read 两向 = **C 进程的
write 流（我要收的）+ C 进程的 read 流（服务端字节 = 我要发的）** ✗ 顶部 8×832B ELF = 启动
加载噪声（fd 复用，无害剔除）。

## 收发两流（按各自方向的时间序 = 状态机直接照推）

**我要收的（C write 流）**：
1. banner 41B（版本+算法）
2. `GOLD\n` → 选模块（Phase0 已实现 ✓）
3. args：`--server\0--sender\0-de.LsfxCIvu\0--list-only\0.\0GOLD/\0` + **`\0\0` 双尾**
4. **`\x1E`**(=30 单字节协议确认) + **checksum 串 30B** `xxh128 xxh3 xxh64 md5 md4 sha1`（无换行 ✗ 读块/读至服务端够用即解析 · 本地探针 TCP 小包稳、真网环境读长 = 记档脆弱点）
5. mux 帧流：`04 00 00 07 00 00 00 00`(8B) → `01 00 00 07 00`(5B) → `03 00 00 07 00 00 00`(7B) → `01 00 00 07 00`(5B)

**我要发的（C read 流 = 我的应答序列）**：
1. banner（Phase0 ✓ `30.0 sha512…`）
2. OK 行（Phase0 ✓）
3. **`81 FE`**(2B · args 双 NUL 之后)
4. **`#` + `xxh128 xxh3 xxh64 md5 md4 sha1 none\n`**（= 客 30B 串 + ` none` 选定形 ✗ 分读 1B#+35B、总 36B · 实现按字面常量与转录逐字节同）+ **`46 19 14 67`**(4B)
5. **57B flist 帧**（= 按树+差分位编码的条目流 · 收到 `04帧` 后发）
6. 尾三帧配对：收 `01(5)` → 发 `01 00 00 07 00`(5) · 收 `03(7)` → 发 `02 00 00 07 00 00`(6) · 收 `01(5)` → 发 `10 00 00 07 00 14 00 00 44 00 00 0d 00 00 01 00 00 00 00 00`(20B · r8 原样照发、若真 CLI 报错按报错修 = 可疑 count/摘要帧记档)

## 差分位方案（r7 编码法 + r8 preserve 解 = 完整）

- **xflags**：`TOP(0x01, 仅首目录)` | `SAME_UID(0x08)` | `SAME_GID(0x10)` **恒置**（`--list-only`
  接收端 `!preserve_uid` → L84 条件短路 = 转录首条 `0x19` 全通 ✗ 后续条目 = `0x18|TOP?`
  （非首无 TOP = `0x18`）+ **SAME_MODE/SAME_TIME 按与前一条差分**（mode/mtime 不同 → 写值）
  + **不使用 SAME_NAME/LONG_NAME**（r8 首版恒发 l2 byte + 全名 ≤255）
- **mode** = **原生 stat mode u32 LE 不做 wire 变换**（golden 两独立命中 `0x41FD` dir /
  `0x101B4` file = stat 原值 = `to_wire_mode` 恒等实证 ✗ 不需要源）· dir=`0o40775`、file=
  `0o100664`（golden 同形；任意 mode 客户端 from_wire 即显 = 我们自己的值 ✓）
- **length** = varlong30(min3) 紧凑编码（r7 法）· dir=4096、file=size_bytes（children meta 源
  ✗ 无 meta fallback=0 记档）· **mtime** = varlong(min4) 紧凑编码（源 = entry.created_at unix
  秒 ✗ 或连接期近似皆可接受 = 客户端显示面、探针验 RC0+文件名）· name = l2 byte + basename
  字节（树序递归：dir 条目 → 子递归 = `.`/文件/子目录… golden 同形）
- **树数据源** = ListCtx{entry_repo, namespace}（bin 构造 = default ns + entry_repo 同 webdav/
  S3 式 ✗ crate 加 vfiles-app/domain path deps = house pattern）

## r9 终清单（实装 + 探针）

rust：依赖加 → ListCtx → OK 后状态机（args 双 NUL 读 → `81 FE` → `1E`+串 → `#串\n`+4B →
`04帧` → **flist 编码器**（差分位 + varlong 紧凑 + mode LE + 递归树流） → 尾三帧配对）→
duplex 整链单测（字节断言含 name 锚）→ #61 三跑 → bin 传 ListCtx → build → **终极探针 =
真 `rsync --list-only rsync://…/files/` = RC0 列出 vfiles 文件**（按 CLI 报错循环修）→
delta（r10）。
