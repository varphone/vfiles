# rsync 协议 daemon（… r19 `-a` 修复 → r20 size-only/window → **r21 filter 规则 flags 形修复**）

## 当前工作树补充（push 接收校验）

- 收端在写入前验证重建长度与 flist 文件长度一致，并校验协商 MD5；校验失败时拒绝该连接，
  不再静默写入损坏内容。
- token 负索引、literal 长度和 basis 区间使用有界/溢出检查，避免畸形 token 导致整数溢出或 panic。
- push 收端的 basis 通过 seekable 流分块计算 checksum，并在匹配 token 时按偏移 seek 读取；
  不再把旧文件整体读入内存。重建结果已直接流入上传存储，不保留整份重建缓冲。
- pull 发送端按流滚动扫描源文件，仅保留一个块窗口；源文件不再与完整 token 流同时驻留内存。
- token 流直接解码到最终文件流，不保留完整 token 副本；协议块 checksum 表仍按文件块数占用内存。

## 状态（r21 末 · `--filter=P/H` 类规则保护修复 ✗ 真机抓出的静默失保）

- **实修 bug**：wire 上规则形如 **`-r <pattern>`**（`get_rule_prefix` = `<+|-><flags…><space><pattern>`
  ✗ flags = s/r/w/n/! 等）；旧解析只 `trim_start()` → pattern 变成 `"r *.probe"` → **永不匹配 →
  `--filter='P *.probe' --delete` 静默删掉本应保护的文件**（真机两轮对照抓到）。
- **修法**：取**第一个空格之后**为 pattern（flags 全剥 ✗ pattern 内空格保留）；`:`（per-dir merge）仍跳过
  （新增 `FILTER-RULE` 原始行日志 = 排障利器）。
- **真机验收（对照式）**：
  | 场景 | 结果 |
  | --- | --- |
  | wire 原始行 | **`-r *.probe`**（日志取证） |
  | 修后 `--filter='P *.probe' --delete` | **`keep.probe` 存活** ✗ 非匹配的 `stray.txt`/`x.m` 正常删除 ✓ |
  | 对照：无 filter 同状态重推 | `keep.probe` 也被删（保护仅由规则生效） ✓ |
- **门禁**：单测新增 flags 形解析 + pattern 含空格断言 × workspace 全绿 × clippy 0 × build。
- **仍债**：per-dir merge（`:` 形，原始行已可 dump）· 符号链接/设备持久化 · 源端仍将完整 token 流暂存在内存 · uid/gid 存储。

## 状态（r20 末 · 快跳判定三档齐全 = 官方 `unchanged_file` 全语义）

- **实装**（快跳判定与官方 `unchanged_file` 逐条对齐）：
  | 开关 | 判定 |
  | --- | --- |
  | 默认 | `size 等 && |Δmtime| <= modify_window`（默认窗口 0 ✗ 跨秒即传） |
  | `--size-only` | 只比 size（**忽略 mtime**） |
  | `--modify-window=N` | mtime 容差 N 秒（`--modify-window=N` 解析 ✗ 非法值退 0） |
  | `-I/--ignore-times` | 全关（一律请求） |
  | `-c/--checksum` | 走整文件校验和（覆盖以上） |
- **真机验收（每例带对照 ✗ `--stats` 数字）**：
  | 场景 | 传输文件数 |
  | --- | --- |
  | 对照：默认 size+mtime，mtime 跨秒 | **1** |
  | `--size-only` 同状态（尺寸同） | **0** |
  | `--modify-window=2`（差 ~1s） | **0** |
  | `--modify-window=0`（差 ~1s） | **1** |
  | `--size-only` + 同尺寸内容变更 | 0（**与官方同义盲区**）· `-c` 则 1 |
  → 拉回 `diff -r` 内容全同。
- **门禁**：`cargo test -p vfiles-rsync` 23/23 × workspace 全绿 × clippy 归零 × fmt × build。
- **债**：符号链接/设备落地（已解析 ✗ domain 无条目类型）· `.rsync-filter` per-dir · basis 流式 ·
  `--numeric-ids` 映射。

## 状态（r19 末 · `rsync -a` 从「完全不可用」到「快跳 + mtime 保真」）

- **修掉的真 bug**：`rsync -a`（**最常用调用**）此前**整条不可用**（此前各轮只测过 `-r`）：
  | 缺口 | 修复 |
  | --- | --- |
  | flist 缺 uid/gid 字段 | `-o`/`-g` 时条目含 `varint uid`/`varint gid`（+ 名字随行 = 仅增量递归）→ 现按官方序解析 |
  | 符号链接 target 位置错 | 官方序在 **uid/gid/rdev 之后**（我们此前紧跟 mode ✗ 无 `-o/-g` 时碰巧正确） |
  | `XMIT_HLINKED` 未处理 | 读走 hlink ndx（`-H` 面 ✗ 防御性） |
  | rdev / atime 未处理 | 设备/特殊文件 `varint30(major)+varint(minor)`；`-U` 的 varlong4 |
  | **uid/gid 名列表** | flist 之后、传输之前：接收端**读走**（`recv_id_list`），发送端**发空表**（`send_id_lists` ✗ 出向须打 MSG_DATA 帧 = 真机 rc12 的坑） |
- **size+mtime 默认快跳**（官方 generator `unchanged_file` 同义）：
  - 新迁移 `0007_entry_source_mtime.sql` + `set_version_source_mtime`（**不动 `create_version` 签名** = 10 处调用零改）
  - push 落盘时记录**源端 mtime**；`EntryChildMeta.source_mtime` 经 `children_with_meta`/`files_with_meta` 带出
  - `RsyncBackend` 增 **`stat`**（连接级 `OnceCell` 缓存 = 一次 `files_with_meta` 覆盖全树 ✗ 非逐文件点查）
  - 判定：`!-I && !-c && size 等 && 源 mtime 等` → 不发请求；`-I/--ignore-times` 全关
- **下载侧 mtime 保真**：`collect_flat` 用 `source_mtime` 优先（回退 `created_at`）→ 客户端 `-a` 拉到**源端 mtime**，
  二次下载客户端自行快跳。
- **真机验收（`--stats` 数字）**：
  | 场景 | 传输文件数 |
  | --- | --- |
  | `-a` 首推 | 3 |
  | 无改动 `-a` 重推 | **0** |
  | `-a -I` 重推 | 3 |
  | `touch` 后 `-a` | 1 |
  | 再无改动 `-a` | 0 |
  | `-a` 拉回后再 `-a` 下载 | **0**（mtime 双侧 1790163085 相等 ✓） |
  | 同秒同尺寸改内容（rsync 经典界） | 0（**与官方一致**）· `-c` 则 1 ✓ |
  | 跨秒改动 | 1，随后 0 ✓ |
  → 拉回 `diff -r` / `find` 结构 + 内容**全同**。
- **门禁**：`cargo test -p vfiles-rsync` **23/23**（新增 uid/gid + id-list 真机帧回归单测）×
  workspace 全绿 × clippy 归零 × fmt × build。诊断开关：`VFILES_RSYNC_DUMP_FLIST=1` 打印收到的 flist。
- **债**：符号链接/设备**落地**（已解析但 domain 无对应条目类型）· `.rsync-filter` per-dir · basis 流式 ·
  `--numeric-ids` 的 id 映射（现一律不映射）。

## 状态（r18 末 · `-c/--checksum` 快跳 = 未变文件 0 传输）

- **实装**：
  | 项 | 内容 |
  | --- | --- |
  | flist 校验和解析 | `-c` 时官方 `send_file_entry` 在**条目末尾**追加 `flist_csum_len`(=16) 字节整文件 MD5（仅普通文件）→ `recv_file_list(…, always_checksum)` 解析入 `FlatEntry.file_sum` |
  | 跳过判定 | 收端（generator 角色）取本地 basis → `MD5(basis) == file_sum` → **不发请求**（等价官方 `generator.c` `memcmp(sum, F_SUM(file), flist_csum_len)`）✗ basis 已在 delta 路径读取 = 零额外 IO |
  | args | `--checksum` / `-c` / `--no-c` |
- **真机验收（`--stats` 数字）**：
  | 场景 | 传输文件数 | Literal |
  | --- | --- | --- |
  | 首推（无 `-c`） | 3 | 1,572,866 |
  | 无改动 `-c` 重推 | **0** | **0** |
  | 改 1 文件后 `-c` | **1** | 1,024（+1,047,552 matched = delta） |
  | 再无改动 `-c` | **0** | — |
  → 拉回 `diff -r` **内容全同**；日志 8 条「校验和一致，跳过」。
- **回归**：`--delete` + `--exclude` 保护 / 无 exclude 删除 / 空目录结构 全保持 ✓。
- **门禁**：`cargo test -p vfiles-rsync` **22/22**（flist `-c` 往返单测新增校验和断言）× workspace
  全绿 × clippy 归零 × fmt × build。
- **债**：**默认快跳（size+mtime）需存源 mtime**（现无该列 = 结构性改动，记档）· 符号链接/设备 ·
  `.rsync-filter` per-dir · basis 流式。

## 状态（r17 末 · 推送空目录落地 = 目录结构完整）

- **实装**：`RsyncBackend` 增 **`mkdir`**（bin = `workspace.create_directory` ✗ 已存在视为成功）
  → push 循环对 flist 的**目录条目显式建目录**（此前 `continue` 跳过 = 空目录丢失；非空目录靠
  文件提交隐式创建故未暴露）。非普通文件（符号链接等）仍跳过（domain 无 symlink kind = 记档债）。
- **真机验收**：
  | 场景 | 结果 |
  | --- | --- |
  | 推含 `empty/` + `nest/a/b/c/`（嵌套空）+ `full/x.txt` 的树 | RC0 |
  | `--list-only` | `empty` / `nest` / `nest/a` / `nest/a/b` / `nest/a/b/c` 全在 ✓ |
  | 拉回 + `find` 结构对比 | **DIR STRUCTURE IDENTICAL** + 内容 IDENTICAL ✓ |
  | 回归：推 / delta / `--delete` | 1,024 literal / 1,047,556 matched；`--delete` 清 `c.txt` + `sub/`；拉回 `diff -r` 同 ✓ |
- **门禁**：`cargo test -p vfiles-rsync` 22/22（FakeBackend 增 `mkdir`）× workspace 全绿 × clippy
  归零 × fmt × build。
- **债**：符号链接/设备（domain 无对应条目类型）· `.rsync-filter` per-dir 规则 · mtime/size 比较 ·
  basis 流式 · `--delete` 的 `P`/`H`/`S`/`R` 规则类型。

## 状态（r16 末 · filter/exclude 规则解析 = `--delete` 可信镜像）

- **实装**（`crates/vfiles-rsync/src/lib.rs`）：`--delete` 场景下客户端会先发 **filter list**，
  规则不再丢弃而是解析为 `FilterRule`：
  - 序列化形（`send_rules`）：`int32 len + "<+|-><flags> <pattern>[/]"`；解析支持 `- pat`（排除/保护）、
    `+ pat`（包含/解除保护）、尾 `/` = 仅目录、首 `/` 或含 `/` = 锚定传输根；其余类型跳过（debug 记）
  - `wildmatch` 子集：`*` 不跨 `/`、`**` 跨 `/`、`?` 单字符、`[..]`/`[!..]` 字符类
  - `is_excluded` = **首条命中定态**（rsync `check_filter` 语义 ✗ 无命中 = 不保护）
  - 保护传播：命中 exclude 的**目录连同整棵子树**不参与删除（祖先前缀集合）
  - `--delete-excluded` → 忽略保护（全删，客户端此时会 elide 规则 ✗ 我们自身标志兜底）
- **真机验收（每例带对照）**：
  | 场景 | 结果 |
  | --- | --- |
  | `--delete`（无 exclude）删本地已删的 `*.tmp` | `removed=2`（对照） ✓ |
  | `--delete --exclude='*.tmp'` | `keep.tmp` **被保护**（无删除日志） ✓ |
  | `--delete --delete-excluded --exclude='*.tmp'` | `removed=2`（保护失效） ✓ |
  | `--delete --exclude='secret/'` | `secret/` 及 `secret/deep/s.txt` **整树保护** ✓ |
  | `--delete`（无 exclude） | `secret/` 整树删除 ✓ |
- **门禁**：`cargo test -p vfiles-rsync` **22/22**（新增 `wildmatch` 12 断言 + 规则解析/保护判定
  9 断言）× workspace 全绿 × clippy 归零 × fmt × build。
- **债**：per-dir merge 文件（`.rsync-filter`）/ `P`/`H`/`S`/`R` 规则类型 / `--filter` 完整语法未支持 ·
  不比较 mtime/size · 符号链接/设备/空目录 · basis 全量入内存。

## 状态（r15 末 · `--delete` 镜像落地 + 后端 trait 化）

- **`--delete` 实装**（`--delete` / `--delete-before/during/delay/after/excluded` / `--del`）：
  1. 识别 args 后**先读 filter list**（客户端 `receiver_wants_list=true` 才会发 ✗ 规则本版忽略）
  2. 收 flist → 传文件（收端 delta 不变）
  3. **镜像**：`backend.list(base, recursive)` 列目标 → 名字不在源集合者 = extra → 一次
     `backend.delete(extras)`（`delete_entries` 处理目录后代）
  4. `--delete` 无 `-r` → 记 warn 跳过（避免过度删除）
- **架构重构**：三个闭包（list/read/write）→ **`RsyncBackend` trait**（`async_trait`：
  `list`/`read`/`write`/`delete` 四法）✗ `handle_conn<S, B: RsyncBackend>` 签名收敛，后续加能力
  不再堆泛型；bin 侧 `RepoBackend` 装配 domain 链，测试侧 `FakeBackend`（内存表）。
- **真机验收**：
  | 场景 | 结果 |
  | --- | --- |
  | 推 3 项 → 删本地 1 项 → **无 `--delete`** 重推 | 目标 `b.txt` **保留**（对照） ✓ |
  | 同状态 **加 `--delete`** 重推 | `b.txt` **消失** + 拉回 `diff -r` **MIRROR IDENTICAL** ✓ |
  | 删整个 `sub/` 后 `--delete` | `sub/` 整树消失 ✓ |
  | 兄弟前缀 `del2/` 隔离 | 未受影响 ✓ |
  | 子路径 `del2/inner/` 内删除 | 只动 `inner/`，`del2/keep.txt` 不动 ✓ |
  | 回归：推/取 delta | 推 1,024 literal / 1,047,552 matched；取同数字 + 重建正确 ✓ |
- **门禁**：`cargo test -p vfiles-rsync` **20/20**（trait 化后 `FakeBackend` 全绿）× workspace 全绿 ×
  clippy 归零 × fmt × build。
- **债**：filter/exclude 规则未解析（`--exclude` + `--delete-excluded` 语义不完整）· `--delete` 无 `-r`
  时跳过 · 不比较 mtime/size · 符号链接/设备 · 空目录不落地 · basis 全量入内存。

## 状态（r14 末 · 收端 delta 落地 = 双向增量，推 3MiB 改 1KB 只传 1.7KB）

- **实装**：push 请求不再恒发 `sum_head` 全零，而是**取本地现有内容作 basis**：
  - `block_size(n)` = `sqrt` 夹取 `[700, 32KiB]`（与官方 `sum_sizes_sqroot` 同量级）
  - `build_block_sums(basis, blength, seed, s2length=16)` = `count/remainder + (弱 sum1 int32 +
    强 MD5(seed‖块)) × count` → 随请求发出
  - 收端把 token 流**原样缓冲**后交 `apply_tokens(tokens, basis, blength, count, remainder)` 重建
    （`>0` literal / `<0` 匹配 basis 块 `idx=-t-1` / `0` 终结）
  - 无 basis（新文件）→ 仍全 literal（回归保持）
- **真机验收（`--stats` 数字 = 证据）**：
  | 场景 | Literal | Matched |
  | --- | --- | --- |
  | 首推（无 basis） | 3,145,728 | 0 |
  | 改 1000B 重推 | **1,773** | **3,143,955（99.94%）** |
  | 内容同重推（`-I`） | **0** | **2,097,152（100%）** |
  | 多次/多文件递归（3 文件改 1 个） | **724** | **1,572,146** |
  → 推后拉回 `cmp` / `diff -r` **全同**（重建逐字节正确 ✓）。
- **门禁**：`cargo test -p vfiles-rsync` **20/20**（新增收端 delta 闭环单测：basis → 块校验和 →
  发送端 token → `apply_tokens` 重建 == 新内容；含无 basis 全 literal 分支）· workspace 全绿 ·
  clippy 归零 · fmt · build。
- **债**：不比较 mtime/size（恒传输 = 等价 `-I`；100% matched 时近零带宽，故影响小）· 符号链接/设备/
  空目录不落地 · `--delete` 未支持 · 收端 basis 全量入内存（大文件内存债）。

## 状态（r13 末 · 认证 + 写门控落地 = 匿名写窗口关闭）

- **实装**：
  | 项 | 内容 |
  | --- | --- |
  | 认证 | rsync **secrets challenge-response**（`@RSYNCD: AUTHREQD <challenge>` → `<user> <base64(HASH(pass‖challenge))>`）：摘要协商 sha512/sha256/md5（banner 同步为 `sha512 sha256 md5`✗ 去掉未实现的 sha1/md4）；**无填充 base64**（真机转录取证 ✗ 带填充必失配）；常量时间比较 |
  | 权限 | `auth users` 条目支持 `:ro` / `:rw` / `:deny`；`VFILES_RSYNC_WRITABLE`（默认 **false**）控制写 |
  | 写门控 | 只读模块收到 push → 多路复用 `MSG_ERROR`（官方 `do_server_recv` 同形）`ERROR: module is read only` + 关闭 |
  | 配置 | `VFILES_RSYNC_WRITABLE` / `VFILES_RSYNC_AUTH_USERS` / `VFILES_RSYNC_SECRETS_FILE`（已入 `.env.example`） |
- **真机验收**：正确口令 list RC0 · 正确口令 push（alice `:rw`）RC0 → 拉回 `diff -r` **IDENTICAL** ·
  错误口令 = `@ERROR: auth failed on module files` + RC5 · bob `:ro` 可 list 但 push 被拒（RC12）·
  匿名 + `writable=false` push 被拒 · 匿名 + `writable=true` push 仍可（显式 opt-in 回归 OK）。
- **门禁**：`cargo test -p vfiles-rsync` **19/19**（新增 sha512 响应真机转录 golden + verify 三态）·
  workspace 全绿 · clippy 归零 · fmt · build。
- **债**：收端无 delta · 不比较 mtime/size · 符号链接/设备/空目录不落地 · `--delete` 未支持 ·
  per-user 授权粒度（现单模块级）。

## 状态（r12 末 · push 打通 = rsync 双向可用 ✗ 推→列→拉 `diff -r` 全同）

- **实装**（`crates/vfiles-rsync/src/lib.rs`）：
  - `recv_file_list` = `recv_file_entry` 逆序解码（xflags varint → `lastname` 前缀压缩重建 →
    length/mtime/mode/symlink ✗ `lastname`/`last_mode`/`last_mtime` 差分态）
  - wire 读端 `data_varint`/`data_varlong`/`data_byte`/`int_byte_extra`（io.c 移植）
  - `handle_conn` 第五参 **`write_file` 闭包**（bin 接 `UploadService`：init + complete_from_stream）
  - `if args.is_sender { download } else { push }` 双径（push 不读 filter list、收 flist、发请求、
    收 literal token 流 + MD5、超时防挂收尾）
- **关键点**：push 的 **ndx = 排序后下标**（复用 `sort_flist` ✗ 不收端排序会请求错文件）；
  请求 `sum_head` 全零 → 客户端**全 literal**（收端不做 delta）。
- **真机验收（推 → 列 → 拉 → `diff -r`）**：
  | 场景 | 结果 |
  | --- | --- |
  | `rsync -r /tmp/src/ …/files/push1/` | RC0 + 日志 `rsync push 完成 files=3` |
  | 推后 `--list-only` | `. / a.txt / tiny.txt / sub`（tree 正确） |
  | 推后拉回 + `diff -r /tmp/src` | **IDENTICAL** |
  | 3MiB 随机 + 深嵌套 + UTF-8 名 → 拉回 `cmp` | 3/3 逐字节同 |
  | 重复推送（幂等） | RC0 |
  | 推后该文件下载 `-I`（r11 delta 回归） | Literal 1,768 / Matched 3,143,960 + 重建正确 |
- **门禁**：`cargo test -p vfiles-rsync` **18/18**（新增 wire 读写互逆、flist 编解码互逆）·
  workspace 全绿 · clippy 归零 · fmt · bin build。
- **入档** `golden/push_wire_r12.md`（双径对照 + 解码序 + 请求/应答帧）。
- **债（明确）**：收端**无 delta**（恒整文件）· 不比较 mtime/size（等价 `-I`）· 符号链接/设备/硬链接
  不落地 · 空目录不创建 · **无认证（匿名可写）** · `--delete` 推送未支持。

## 状态（r11 末 · 真 delta 落地 = 增量重建逐字节正确 ✗ 弱 sum1 + 强 sum2 全实证）

- **实装**（`crates/vfiles-rsync/src/lib.rs`）：`BlockSum{sum1,sum2,len}`（接收端块校验和不再丢弃）
  → `build_delta_tokens`（弱滚动命中 → 强校验确认 → 匹配 token `-(idx+1)`；未命中段 literal
  `int32(len)+data`；`int32(0)` 终结）· `checksum1_signed`（**signed char** 滚动）·
  `md5_seeded`（块强校验和 = `MD5(seed_le‖块)` 前 s2length 字节）· `emit_literal`（≤CHUNK_SIZE 分块）。
- **校验和两式辨析（真机实证 ✗ 入档 `golden/delta_checksum_r11.md`）**：
  | 用途 | 公式 |
  | --- | --- |
  | 块强校验和（delta 匹配） | `MD5(seed_le ‖ block)` 前 `s2length` 字节（首轮 s2length=2） |
  | 整文件校验和（收尾） | `MD5(内容)` **无 seed**（streaming `sum_update` 路径） |
  弱校验和 = `(s1 & 0xffff) | (s2 << 16)`，`s1=Σ(i8)b`、`s2=Σs1` ✗ `0x760cfeda` 逐位命中。
- **真机验收（`--stats` 数字 = delta 证据）**：
  | 场景（2,254,652B CSV） | Literal | Matched |
  | --- | --- | --- |
  | 首轮（无 basis） | 2,254,652 | 0 |
  | 改 1000B 后 `-I` | **2,992** | **2,251,660（99.87%）** |
  | 再跑 `-I`（内容同） | **0** | **2,254,652（100%）** |
  | 递归树 4 文件改 500B `-I` | **1,448** | **70,688,460** |
  → 落地内容 **SHA-256 全部 = blobs 表**（delta 重建逐字节正确 ✓）。
- **门禁**：`cargo test -p vfiles-rsync` **16/16**（新增：真机校验和逐位 golden、全匹配 token 流、
  部分匹配 literal/匹配混合）× workspace 全绿 × clippy 归零 × fmt 干净 × bin build 绿。
- **r12 清单**：收端 push（复用 upload 链）→ secrets 认证 → 大文件流式（现全量入内存）→
  `--checksum`/压缩面。

## 状态（r10 末 · 整文件下载落地 = rsync 从"能列"到"能取" ✗ 真机内容校验全绿）

- **实装兑现（下载探针 ✔）**：真 `rsync 3.2.7` 对我方 daemon：
  | 探针 | 结果 |
  | --- | --- |
  | `rsync …/files/说明.md /tmp/`（单文件） | RC0 + 内容 `v2 content` ✓ |
  | `rsync -r …/files/aka/foo/bar/hallo/ /tmp/`（4 文件 70MB 嵌套） | RC0 + **4/4 SHA-256 与 blobs 表逐字节同** ✓ |
  | 同上二跑 `-I`（强制 basis → 接收端发**块校验和** count>0） | RC0 + 4/4 哈希仍全对 ✓ |
- **协议实现**（照 `golden/download_wire_r9.md` 零猜）：
  1. **checksum 清单收敛 `"md5"`** = 双方必选 md5（照抄官方全清单会协商 xxh128 ✗ 需 XXH3-128）
  2. `send_files` 循环：`read_ndx` → `iflags`(shortint) → [BASIS_TYPE 1B] → [XNAME vstring] →
     仅 `ITEM_TRANSFER` 读 `sum_head`(4×int32) + 块校验和（**读后即弃** ✗ 全 literal 对 count>0 亦合法）
     → `write_ndx_and_attrs` 回显 + `write_sum_head` 回显 → `int32(len)+data` 分块(CHUNK_SIZE 32KB)
     → `int32(0)` 终结 → **`MD5(内容)` 16B**（真机实证无 seed）
  3. **ndx 对齐**：`sort_flist` 按 rsync `f_name_cmp` 真机序（**同目录文件先于子目录、各按名升序、
     遇目录深度优先下钻** ✗ 官方 `--list-only` 输出序实证）重排后再编码 = 接收端排序下标一致
  4. 单文件请求：flist 仅该文件（name = basename、无 `.`），size 取父目录批量 meta
- **新增**：`FlatEntry.fs_path`（命名空间读取路径）+ `handle_conn` 第 4 参 `read_file` 闭包
  （bin 侧 = `workspace.read_file_bytes` 与 WebDAV/S3 同源 blob 链）+ `write_ndx`/`data_int`/
  `data_shortint`/`data_vstring`/`md5_digest` + `md-5 0.10` 依赖（本机 registry 已有 = 离线可加）。
- **门禁**：`cargo test -p vfiles-rsync` **13/13**（排序黄金 + 整文件下载整链逐字节 + md5 RFC 向量
  + 既有 10 项）× `cargo test --workspace` 全绿 × clippy/fmt × bin build。
- **r11 清单**：真 **delta**（rolling checksum + strong 匹配 ✗ 现全 literal = 正确但无增量收益）
  → 收端 push（复用 upload 链）→ secrets 认证 → `--checksum`/压缩面 → 会话内 `read_file` 改流式
  （现全量入内存 = 大文件内存债）。

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
