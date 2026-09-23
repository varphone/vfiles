//! rsync daemon（协议 30 · 权威源驱动实装 ✗ 全部字节形来自官方源码与真机转录）。
//!
//! 分层：
//! - **wire 原语**：`write_varint` / `write_varlong` / `write_vstring` / mux 帧（io.c 原样移植）
//! - **flist 编码**：`FlatEntry` → 字节流（flist.c `send_file_entry` 字段序 + XMIT 位表）
//! - **L1 `handle_conn`**：daemon 握手 → args → setup_protocol（compat_flags / 校验和协商 /
//!   seed）→ filter list → flist → NDX 收尾。数据源以闭包注入 = duplex 单测零 domain 依赖。
//! - **L2 `collect_flat`**：domain 树 → `FlatEntry`（bin 使用）。
//!
//! 权威依据：官方 rsync 3.2.7 源码（`io.c`/`flist.c`/`compat.c`/`clientserver.c`/`main.c`）+
//! `golden/` 转录。关键事实（r9 更正 r7/r8 的猜测）：
//! 1. mux 头 = `[len_lo, len_mid, len_hi, MPLEX_BASE+MSG_DATA]`，**len = payload 长度**（≤16MB）
//! 2. 协议 ≥30 时服务端先 `write_varint(compat_flags)`；`CF_VARINT_FLIST_FLAGS`（客户端 `-e` 串
//!    含 `v`）决定 flist xflags 用 **varint** 还是 byte/shortint
//! 3. 校验和协商 = 双向 vstring（官方未显式回选，双方各自取最强交集）
//! 4. `46 19 14 67` 不是常量 = `checksum_seed`（time^pid），必须随机生成
//! 5. flist 字段序 = xflags → l2 → name → length(varlong3) → mtime(varlong4) → mode(LE4)；
//!    收尾 = `write_end_of_flist`（varint 模式 = `00 00`）
//! 6. 收尾对答 = 3×NDX_DONE + 5×varlong30(3) 统计；随后读最后一个 NDX_DONE 问候
//! 7. 递归 = 单 flist 全量（**不置 CF_INC_RECURSE** ✗ 避开增量递归面），嵌套名 = 全相对路径

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use vfiles_domain::{EntryKind, NamespaceId};

/// daemon banner（版本 + 算法串 ✗ 官方 `output_daemon_greeting` 形）。
pub const PROTOCOL_LINE: &str = "@RSYNCD: 30.0 sha512 sha256 md5\n";
/// 服务端校验和清单。**只列 `md5`** = 双方必收敛到 md5（照抄官方全清单会协商出 xxh128，
/// 需 XXH3-128 实现 ✗ 见 `golden/download_wire_r9.md`：文件校验和 = `MD5(内容)` 无 seed）。
const CHECKSUM_LIST: &str = "md5";
/// 压缩协商清单（本实现不做压缩 = 仅 `none`，客户端协商后自降级为无压缩）。
const COMPRESS_LIST: &str = "none";

const MPLEX_BASE: u8 = 7;
const MSG_DATA: u8 = 0;

// XMIT 位（rsync.h）
const XMIT_TOP_DIR: u16 = 1 << 0;
const XMIT_SAME_MODE: u16 = 1 << 1;
const XMIT_EXTENDED_FLAGS: u16 = 1 << 2;
const XMIT_SAME_UID: u16 = 1 << 3;
const XMIT_SAME_GID: u16 = 1 << 4;
const XMIT_SAME_TIME: u16 = 1 << 7;

// compat_flags（compat.c）
const CF_SYMLINK_TIMES: u32 = 1 << 1;
const CF_SYMLINK_ICONV: u32 = 1 << 2;
const CF_SAFE_FLIST: u32 = 1 << 3;
const CF_AVOID_XATTR_OPTIM: u32 = 1 << 4;
const CF_CHKSUM_SEED_FIX: u32 = 1 << 5;
const CF_INPLACE_PARTIAL_DIR: u32 = 1 << 6;
const CF_VARINT_FLIST_FLAGS: u32 = 1 << 7;
const CF_ID0_NAMES: u32 = 1 << 8;

const NDX_DONE: i32 = -1;

// ITEM 位（rsync.h ✗ 接收端请求帧使用）
const ITEM_BASIS_TYPE_FOLLOWS: u16 = 1 << 11;
const ITEM_XNAME_FOLLOWS: u16 = 1 << 12;
const ITEM_TRANSFER: u16 = 1 << 15;
/// ITEM_IS_NEW（新文件标记 ✗ push 请求用）。
const ITEM_IS_NEW: u16 = 1 << 13;
/// MSG_NO_SEND（rsync.h msgcode ✗ 文件读取失败时通知接收端）。
const MSG_NO_SEND: u8 = 102;
/// 字面量分块（rsync.h `CHUNK_SIZE`）。
const CHUNK_SIZE: usize = 32 * 1024;

// ─────────────────────────── wire 原语（io.c 移植）───────────────────────────

/// varint（io.c `write_varint` 逐行移植 ✗ 32 位小端 + 紧凑前缀）。
pub fn write_varint(x: i32, out: &mut Vec<u8>) {
    let mut b = [0u8; 5];
    b[1..5].copy_from_slice(&(x as u32).to_le_bytes());
    let mut cnt = 4usize;
    while cnt > 1 && b[cnt] == 0 {
        cnt -= 1;
    }
    let bit: u8 = 1u8 << (7 - cnt + 1);
    if b[cnt] >= bit {
        cnt += 1;
        b[0] = !(bit - 1);
    } else if cnt > 1 {
        b[0] = b[cnt] | !(bit.wrapping_mul(2).wrapping_sub(1));
    } else {
        b[0] = b[1];
    }
    out.extend_from_slice(&b[0..cnt]);
}

/// varlong(min)（io.c `write_varlong` 逐行移植 ✗ 64 位小端 + 紧凑前缀）。
pub fn write_varlong(min: usize, x: i64, out: &mut Vec<u8>) {
    assert!((1..=8).contains(&min), "varlong min 1..=8");
    let mut b = [0u8; 9];
    b[1..9].copy_from_slice(&(x as u64).to_le_bytes());
    let mut cnt = 8usize;
    while cnt > min && b[cnt] == 0 {
        cnt -= 1;
    }
    let bit: u8 = 1u8 << (7 - cnt + min);
    if b[cnt] >= bit {
        cnt += 1;
        b[0] = !(bit - 1);
    } else if cnt > min {
        b[0] = b[cnt] | !(bit.wrapping_mul(2).wrapping_sub(1));
    } else {
        b[0] = b[cnt];
    }
    out.extend_from_slice(&b[0..cnt]);
}

/// vstring（io.c `write_vstring`：len ≤0x7F 单字节，否则高字节 |0x80 + 低字节）。
pub fn write_vstring(s: &str, out: &mut Vec<u8>) {
    let len = s.len();
    assert!(len <= 0x7FFF, "vstring 长度上限");
    if len > 0x7F {
        out.push((len / 0x100 + 0x80) as u8);
    }
    out.push((len & 0xFF) as u8);
    out.extend_from_slice(s.as_bytes());
}

/// 单条 MSG_DATA mux 帧（`[len_lo, len_mid, len_hi, 7] + payload`）。
pub fn mux_frame(payload: &[u8]) -> Vec<u8> {
    assert!(payload.len() <= 0xFF_FFFF, "单 MSG_DATA 帧 ≤16MB");
    let len = payload.len();
    let mut out = Vec::with_capacity(len + 4);
    out.push((len & 0xFF) as u8);
    out.push(((len >> 8) & 0xFF) as u8);
    out.push(((len >> 16) & 0xFF) as u8);
    out.push(MPLEX_BASE + MSG_DATA);
    out.extend_from_slice(payload);
    out
}

// ─────────────────────────── 数据模型与 flist 编码 ───────────────────────────

/// 平铺条目（L2 输出 ✗ L1 编码消费）。
#[derive(Debug, Clone, PartialEq)]
pub struct FlatEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub mtime: i64,
    /// 原生 stat mode（`to_wire_mode` 在 Linux 上恒等 ✗ 直传 LE4）。
    pub mode: u32,
    /// 命名空间内读取路径（下载用 ✗ `.` = 请求基准目录；测试/列表可留空）。
    pub fs_path: String,
    /// `-c/--checksum` 时 flist 携带的整文件 MD5（16B ✗ 其余情况 None）。
    pub file_sum: Option<Vec<u8>>,
}

impl FlatEntry {
    /// 目录条目（mode = drwxr-xr-x）。
    pub fn dir(name: impl Into<String>, mtime: i64) -> Self {
        FlatEntry {
            name: name.into(),
            is_dir: true,
            size: 4096,
            mtime,
            mode: 0o40755,
            fs_path: String::new(),
            file_sum: None,
        }
    }

    /// 文件条目（mode = -rw-r--r--）。
    pub fn file(name: impl Into<String>, size: u64, mtime: i64) -> Self {
        FlatEntry {
            name: name.into(),
            is_dir: false,
            size,
            mtime,
            mode: 0o100644,
            fs_path: String::new(),
            file_sum: None,
        }
    }

    /// 设置命名空间读取路径（builder）。
    pub fn with_fs_path(mut self, path: impl Into<String>) -> Self {
        self.fs_path = path.into();
        self
    }
}

/// 列表请求（args 解析结果 ✗ 闭包入参）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRequest {
    /// `-r` / `--recursive`。
    pub recursive: bool,
    /// 模块内相对路径（"" = 模块根）。
    pub path: String,
}

/// flist 字节编码（flist.c `send_file_entry` 字段序 + r9 真机转录逐字节校准）：
/// xflags → l2 → name（恒发全名，不用 SAME_NAME）→ length(varlong3) →
/// [mtime varlong4 if !SAME_TIME] → [mode LE4 if !SAME_MODE]；收尾 `00 00`(varint) 或 `00`。
///
/// xflags 恒置 `SAME_UID|SAME_GID`（list-only 无 `-o/-g` = 官方同形）+ 首条 `.` 置 `TOP_DIR`；
/// `SAME_MODE`/`SAME_TIME` 按与前一条差分。**不置 NO_CONTENT_DIR**（真机转录两例均无此位）。
pub fn encode_flist(entries: &[FlatEntry], varint_flags: bool) -> Vec<u8> {
    let mut body = Vec::new();
    let mut last_mode: Option<u32> = None;
    let mut last_mtime: Option<i64> = None;
    for e in entries {
        assert!(e.name.len() <= 255, "LONG_NAME 未实现：name ≤255 字节");
        let mut x: u16 = XMIT_SAME_UID | XMIT_SAME_GID;
        if e.is_dir && e.name == "." {
            x |= XMIT_TOP_DIR;
        }
        if last_mode == Some(e.mode) {
            x |= XMIT_SAME_MODE;
        }
        if last_mtime == Some(e.mtime) {
            x |= XMIT_SAME_TIME;
        }
        last_mode = Some(e.mode);
        last_mtime = Some(e.mtime);

        if varint_flags {
            write_varint(x as i32, &mut body);
        } else if (x & 0xFF00) != 0 || x == 0 {
            let x = if x == 0 { XMIT_EXTENDED_FLAGS } else { x };
            body.push((x & 0xFF) as u8);
            body.push((x >> 8) as u8);
        } else {
            body.push(x as u8);
        }

        // 全名（l1 = 0 ✗ 不用 SAME_NAME/LONG_NAME，≤255 l2 单字节）
        let name = e.name.as_bytes();
        body.push(name.len() as u8);
        body.extend_from_slice(name);

        write_varlong(3, e.size as i64, &mut body);
        if x & XMIT_SAME_TIME == 0 {
            write_varlong(4, e.mtime, &mut body);
        }
        if x & XMIT_SAME_MODE == 0 {
            body.extend_from_slice(&e.mode.to_le_bytes());
        }
        // `-c`：普通文件末尾附整文件校验和（与官方 send_file_entry 同位）
        if let Some(sum) = &e.file_sum {
            body.extend_from_slice(sum);
        }
    }
    // write_end_of_flist：xfer_flags_as_varint → varint(0)+varint(0)；否则单字节 0
    if varint_flags {
        body.push(0);
        body.push(0);
    } else {
        body.push(0);
    }
    body
}

/// 按 rsync `f_name_cmp` 语义排序 flist（ndx = 排序后下标 ✗ 不排序会发错文件）。
///
/// 真机实证（官方 daemon `--list-only` 输出序）：**同一目录下文件在前、子目录在后，各自按名字
/// 升序；遇到子目录立即深度优先下钻**。`.`（根）恒首。实现 = 按父路径建树后 DFS。
pub fn sort_flist(entries: &mut Vec<FlatEntry>) {
    use std::collections::BTreeMap;
    let mut children: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut root_dot: Option<usize> = None;
    for (i, e) in entries.iter().enumerate() {
        if e.name == "." {
            root_dot = Some(i);
            continue;
        }
        let parent = match e.name.rfind('/') {
            Some(p) => e.name[..p].to_string(),
            None => String::new(),
        };
        children.entry(parent).or_default().push(i);
    }

    fn basename(e: &FlatEntry) -> &str {
        e.name.rsplit('/').next().unwrap_or("")
    }

    fn emit(
        entries: &[FlatEntry],
        children: &BTreeMap<String, Vec<usize>>,
        parent: &str,
        order: &mut Vec<usize>,
    ) {
        let Some(kids) = children.get(parent) else {
            return;
        };
        let mut files: Vec<usize> = kids
            .iter()
            .copied()
            .filter(|&i| !entries[i].is_dir)
            .collect();
        let mut dirs: Vec<usize> = kids
            .iter()
            .copied()
            .filter(|&i| entries[i].is_dir)
            .collect();
        files.sort_by(|&a, &b| basename(&entries[a]).cmp(basename(&entries[b])));
        dirs.sort_by(|&a, &b| basename(&entries[a]).cmp(basename(&entries[b])));
        for i in files {
            order.push(i);
        }
        for i in dirs {
            order.push(i);
            let child = entries[i].name.clone();
            emit(entries, children, &child, order);
        }
    }

    let mut order = Vec::with_capacity(entries.len());
    if let Some(i) = root_dot {
        order.push(i);
    }
    emit(entries, &children, "", &mut order);
    // 兜底：未纳入树序的条目（异常输入）按原序追加
    for i in 0..entries.len() {
        if !order.contains(&i) {
            order.push(i);
        }
    }

    let mut slots: Vec<Option<FlatEntry>> = entries.drain(..).map(Some).collect();
    let mut sorted = Vec::with_capacity(order.len());
    for i in order {
        if let Some(e) = slots[i].take() {
            sorted.push(e);
        }
    }
    *entries = sorted;
}

// ─────────────────────────── filter/exclude 规则（`--delete` 保护面）───────────────────────────

/// 一条接收端 filter 规则（`--exclude` / `--include` 序列化形 `- pat` / `+ pat`）。
#[derive(Debug, Clone)]
struct FilterRule {
    /// true = include（解除保护），false = exclude（保护不被删除）。
    include: bool,
    /// 仅目录（pattern 尾 `/`）。
    dir_only: bool,
    /// 锚定传输根（pattern 首 `/` 或含 `/`）。
    anchored: bool,
    pattern: String,
}

/// 解析序列化规则行（`+ pat/` / `- pat` ✗ 其余类型跳过）。
fn parse_rule(line: &str) -> Option<FilterRule> {
    let line = line.trim_end_matches(['\n', '\r']);
    let (include, rest) = match line.as_bytes().first()? {
        b'+' => (true, &line[1..]),
        b'-' => (false, &line[1..]),
        _ => return None,
    };
    // 形如 `<+|-><flags…><space><pattern>`（flags = s/r/w/n/! 等 ✗ get_rule_prefix 恒带空格分隔）
    let rest = match rest.split_once(' ') {
        Some((_, pattern)) => pattern,
        None => rest.trim_start(),
    };
    let (pat, dir_only) = match rest.strip_suffix('/') {
        Some(p) => (p, true),
        None => (rest, false),
    };
    let anchored = pat.starts_with('/');
    let pattern = pat.trim_start_matches('/').to_string();
    if pattern.is_empty() {
        return None;
    }
    Some(FilterRule {
        include,
        dir_only,
        anchored: anchored || pattern.contains('/'),
        pattern,
    })
}

/// 通配匹配（rsync wildmatch 子集）：`*` 不跨 `/`、`**` 跨 `/`、`?` 单字符、`[..]` 字符类。
fn wildmatch(pattern: &str, text: &str) -> bool {
    fn m(p: &[u8], t: &[u8]) -> bool {
        if p.is_empty() {
            return t.is_empty();
        }
        match p[0] {
            b'*' => {
                let double = p.len() > 1 && p[1] == b'*';
                let rest = if double { &p[2..] } else { &p[1..] };
                if double {
                    (0..=t.len()).any(|i| m(rest, &t[i..]))
                } else {
                    let mut i = 0;
                    loop {
                        if m(rest, &t[i..]) {
                            return true;
                        }
                        if i >= t.len() || t[i] == b'/' {
                            return false;
                        }
                        i += 1;
                    }
                }
            }
            b'?' => !t.is_empty() && t[0] != b'/' && m(&p[1..], &t[1..]),
            b'[' => {
                let Some(close) = p.iter().position(|&c| c == b']') else {
                    return !t.is_empty() && t[0] == b'[' && m(&p[1..], &t[1..]);
                };
                if t.is_empty() || t[0] == b'/' {
                    return false;
                }
                let class = &p[1..close];
                let (negate, class) = match class.first() {
                    Some(b'!') | Some(b'^') => (true, &class[1..]),
                    _ => (false, class),
                };
                let mut hit = false;
                let mut i = 0;
                while i < class.len() {
                    if i + 2 < class.len() && class[i + 1] == b'-' {
                        if class[i] <= t[0] && t[0] <= class[i + 2] {
                            hit = true;
                        }
                        i += 3;
                    } else {
                        if class[i] == t[0] {
                            hit = true;
                        }
                        i += 1;
                    }
                }
                hit != negate && m(&p[close + 1..], &t[1..])
            }
            c => !t.is_empty() && t[0] == c && m(&p[1..], &t[1..]),
        }
    }
    m(pattern.as_bytes(), text.as_bytes())
}

/// 首条命中规则定保护态（rsync `check_filter` 语义 ✗ 无命中 = 不保护）。
fn is_excluded(rules: &[FilterRule], rel_path: &str, is_dir: bool) -> bool {
    let base = rel_path.rsplit('/').next().unwrap_or(rel_path);
    for r in rules {
        if r.dir_only && !is_dir {
            continue;
        }
        let target = if r.anchored { rel_path } else { base };
        if wildmatch(&r.pattern, target) {
            return !r.include;
        }
    }
    false
}

// ─────────────────────────── daemon 认证（secrets ✗ r8）───────────────────────────

/// 本端支持的认证摘要（最强优先 ✗ 与 banner 一致）。
const AUTH_DIGESTS: [&str; 3] = ["sha512", "sha256", "md5"];

/// rsync daemon 认证配置（空 users = 匿名访问；writable 控制 push）。
#[derive(Debug, Default, Clone)]
pub struct AuthConfig {
    /// `auth users` 条目（可带 `:ro` / `:rw` / `:deny`）。
    pub users: Vec<String>,
    /// user → password（来自 secrets 文件）。
    pub secrets: std::collections::HashMap<String, String>,
    /// 模块是否可写（false = 拒收 push）。
    pub writable: bool,
}

impl AuthConfig {
    /// 是否需要认证。
    pub fn required(&self) -> bool {
        !self.users.is_empty()
    }

    /// `user[:opts]` 拆分。
    fn split_user(entry: &str) -> (&str, &str) {
        match entry.split_once(':') {
            Some((n, o)) => (n.trim(), o.trim()),
            None => (entry.trim(), ""),
        }
    }

    /// 校验用户 + 响应值 → `Some(该用户是否可写)`；`None` = 拒绝。
    fn verify(&self, user: &str, response: &str, challenge: &str, digest: &str) -> Option<bool> {
        let entry = self.users.iter().find(|e| Self::split_user(e).0 == user)?;
        let (_, opts) = Self::split_user(entry);
        if opts == "deny" {
            return None;
        }
        let pass = self.secrets.get(user)?;
        let expect = auth_response(pass, challenge, digest);
        if !constant_time_eq(expect.as_bytes(), response.as_bytes()) {
            return None;
        }
        Some(match opts {
            "ro" => false,
            "rw" => true,
            _ => self.writable,
        })
    }
}

/// 响应值 = base64(HASH(password ‖ challenge))（authenticate.c `generate_hash`）。
fn auth_response(password: &str, challenge: &str, digest: &str) -> String {
    use base64::Engine;
    let mut input = Vec::with_capacity(password.len() + challenge.len());
    input.extend_from_slice(password.as_bytes());
    input.extend_from_slice(challenge.as_bytes());
    base64::engine::general_purpose::STANDARD_NO_PAD.encode(hash_bytes(digest, &input))
}

/// 摘要计算（sha512 / sha256 / md5 ✗ 其余回退 md5）。
fn hash_bytes(digest: &str, data: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256, Sha512};
    match digest {
        "sha512" => Sha512::digest(data).to_vec(),
        "sha256" => Sha256::digest(data).to_vec(),
        _ => md5_digest(data).to_vec(),
    }
}

/// 常量时间字节比较（避免响应值计时侧信道）。
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// 从 greeting 行取客户端摘要清单（`@RSYNCD: 31.0 sha512 sha256 …`）。
fn parse_auth_list(line: &str) -> Vec<String> {
    let rest = line.trim().strip_prefix("@RSYNCD:").unwrap_or("").trim();
    rest.split_whitespace()
        .skip(1)
        .map(|t| t.to_string())
        .collect()
}

/// 双方最强交集（本端清单优先序）。
fn pick_digest(client: &[String]) -> &'static str {
    AUTH_DIGESTS
        .iter()
        .find(|d| client.iter().any(|c| c == *d))
        .copied()
        .unwrap_or("md5")
}

/// 随机 challenge（base64(24B urandom) ✗ 客户端仅回读该串，不回算）。
fn gen_challenge() -> String {
    use base64::Engine;
    let mut buf = [0u8; 24];
    if std::fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut buf))
        .is_err()
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let seed = (now ^ ((std::process::id() as u128) << 40)).to_le_bytes();
        buf[..16].copy_from_slice(&seed);
    }
    base64::engine::general_purpose::STANDARD_NO_PAD.encode(buf)
}

/// 客户端 args 解析结果。
#[derive(Debug, Default, Clone)]
struct ParsedArgs {
    is_sender: bool,
    recursive: bool,
    compress: bool,
    list_only: bool,
    /// `--delete*`（镜像：删目标端源端没有的条目）。
    delete: bool,
    /// `--delete-excluded`（连被排除项一起删）。
    delete_excluded: bool,
    /// `-c/--checksum`（flist 携带整文件校验和 → 一致即跳过）。
    checksum: bool,
    /// `-I/--ignore-times`（关闭 size+mtime 快跳）。
    ignore_times: bool,
    /// `--size-only`（只比大小 ✗ 忽略 mtime）。
    size_only: bool,
    /// `--modify-window=N`（mtime 容差秒数 ✗ 官方同义）。
    modify_window: i64,
    /// `-o/--owner`、`-g/--group`、`-D/--devices`/`--specials`、`-U/--atimes`（flist 可选字段）。
    preserve_uid: bool,
    preserve_gid: bool,
    preserve_devices: bool,
    preserve_specials: bool,
    preserve_atimes: bool,
    /// `--numeric-ids`（不传 uid/gid 名列表）。
    numeric_ids: bool,
    /// `-e` 选项值 = 官方 `client_info`（行为能力串，决定 compat_flags）。
    client_info: String,
    paths: Vec<String>,
}

/// NUL 分隔 args → 结构化（`--server`/`--sender`/短标志簇/长选项/路径）。
fn parse_args(segs: &[String]) -> ParsedArgs {
    let mut a = ParsedArgs::default();
    for seg in segs {
        if seg == "--server" {
            continue;
        }
        if seg == "--sender" {
            a.is_sender = true;
            continue;
        }
        if let Some(long) = seg.strip_prefix("--") {
            match long {
                "list-only" => a.list_only = true,
                "recursive" => a.recursive = true,
                "no-r" => a.recursive = false,
                "delete" | "delete-before" | "delete-during" | "delete-delay" | "delete-after"
                | "del" => a.delete = true,
                "delete-excluded" => {
                    a.delete = true;
                    a.delete_excluded = true;
                }
                "checksum" => a.checksum = true,
                "no-c" => a.checksum = false,
                "ignore-times" => a.ignore_times = true,
                "no-ignore-times" => a.ignore_times = false,
                "size-only" => a.size_only = true,
                "no-size-only" => a.size_only = false,
                _ if long.starts_with("modify-window=") => {
                    a.modify_window = long["modify-window=".len()..].parse().unwrap_or(0);
                }
                "owner" => a.preserve_uid = true,
                "no-owner" | "no-o" => a.preserve_uid = false,
                "group" => a.preserve_gid = true,
                "no-group" | "no-g" => a.preserve_gid = false,
                "devices" => a.preserve_devices = true,
                "specials" => a.preserve_specials = true,
                "no-devices" | "no-D" => {
                    a.preserve_devices = false;
                    a.preserve_specials = false;
                }
                "atimes" => a.preserve_atimes = true,
                "no-atimes" => a.preserve_atimes = false,
                "numeric-ids" => a.numeric_ids = true,
                "no-numeric-ids" => a.numeric_ids = false,
                _ => {}
            }
            continue;
        }
        if seg.len() > 1 && seg.starts_with('-') {
            let chars = &seg[1..];
            let bytes = chars.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                match bytes[i] as char {
                    // `-e` 取余下全部为参数（官方 maybe_add_e_option 尾附 = 必在簇末）
                    'e' => {
                        a.client_info = chars[i + 1..].to_string();
                        i = bytes.len();
                    }
                    'r' => a.recursive = true,
                    'z' => a.compress = true,
                    'c' => a.checksum = true,
                    'I' => a.ignore_times = true,
                    'o' => a.preserve_uid = true,
                    'g' => a.preserve_gid = true,
                    'D' => {
                        a.preserve_devices = true;
                        a.preserve_specials = true;
                    }
                    'U' => a.preserve_atimes = true,
                    _ => {}
                }
                i += 1;
            }
            continue;
        }
        a.paths.push(seg.clone());
    }
    a
}

/// 从 args 路径段提取模块内相对路径（`files/sub/` → `sub`，`files/` → ""）。
fn module_path(paths: &[String], module: &str) -> String {
    let raw = paths.last().cloned().unwrap_or_default();
    if raw == "." {
        return String::new();
    }
    let p = raw.trim_start_matches('/');
    let p = p.strip_prefix(module).unwrap_or(p);
    p.trim_matches('/').to_string()
}

/// 官方 `setup_protocol` 的 compat_flags 计算（服务端侧 ✗ 不置 INC_RECURSE = 单 flist 全量）。
fn compute_compat(client_info: &str) -> u32 {
    let mut cf = CF_SYMLINK_TIMES | CF_SYMLINK_ICONV;
    let has = |c: char| client_info.contains(c);
    if has('f') {
        cf |= CF_SAFE_FLIST;
    }
    if has('x') {
        cf |= CF_AVOID_XATTR_OPTIM;
    }
    if has('C') {
        cf |= CF_CHKSUM_SEED_FIX;
    }
    if has('I') {
        cf |= CF_INPLACE_PARTIAL_DIR;
    }
    if has('v') {
        cf |= CF_VARINT_FLIST_FLAGS;
    }
    if has('u') {
        cf |= CF_ID0_NAMES;
    }
    cf
}

/// 解析 `@RSYNCD: 30.0` → 主版本。
fn parse_version(line: &str) -> Option<u32> {
    let rest = line.trim().strip_prefix("@RSYNCD:")?.trim();
    rest.split('.').next()?.parse().ok()
}

/// checksum_seed（官方 `time(NULL) ^ (getpid()<<6)` ✗ 无需密码学强度，仅需对端回读一致）。
fn make_seed() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let pid = std::process::id() as u64;
    ((now ^ (pid << 6)) & 0xFFFF_FFFF) as u32
}

// ─────────────────────────── 连接 I/O ───────────────────────────

async fn write_raw<S>(rw: &mut BufReader<S>, bytes: &[u8]) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    rw.get_mut().write_all(bytes).await?;
    rw.get_mut().flush().await
}

async fn read_raw_exact<S>(rw: &mut BufReader<S>, n: usize) -> std::io::Result<Vec<u8>>
where
    S: AsyncRead + Unpin,
{
    let mut buf = vec![0u8; n];
    rw.read_exact(&mut buf).await?;
    Ok(buf)
}

/// 原始行读（含 `\n`）。
async fn read_raw_line<S>(rw: &mut BufReader<S>) -> std::io::Result<String>
where
    S: AsyncRead + Unpin,
{
    let mut line = String::new();
    rw.read_line(&mut line).await?;
    Ok(line)
}

/// 读客户端 checksum/compress 协商 vstring（原始流）。
async fn read_vstring<S>(rw: &mut BufReader<S>) -> std::io::Result<String>
where
    S: AsyncRead + Unpin,
{
    let first = read_raw_exact(rw, 1).await?[0];
    let len = if first & 0x80 != 0 {
        ((first & 0x7F) as usize) * 0x100 + read_raw_exact(rw, 1).await?[0] as usize
    } else {
        first as usize
    };
    if len == 0 {
        return Ok(String::new());
    }
    let bytes = read_raw_exact(rw, len).await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// 从 mux 输入流取恰好 n 字节（跳过非 MSG_DATA 消息 = 记日志）。
async fn data_take<S>(
    rw: &mut BufReader<S>,
    pending: &mut Vec<u8>,
    n: usize,
) -> std::io::Result<Vec<u8>>
where
    S: AsyncRead + Unpin,
{
    while pending.len() < n {
        let hdr = read_raw_exact(rw, 4).await?;
        let len = (hdr[0] as usize) | ((hdr[1] as usize) << 8) | ((hdr[2] as usize) << 16);
        let tag = hdr[3];
        let payload = read_raw_exact(rw, len).await?;
        if tag == MPLEX_BASE + MSG_DATA {
            pending.extend_from_slice(&payload);
        } else {
            tracing::debug!(tag = tag, len = len, "rsync：忽略非 MSG_DATA 消息");
        }
    }
    Ok(pending.drain(0..n).collect())
}

/// 协议 30 的 NDX 解码（io.c `read_ndx` 逐行移植）。
async fn read_ndx<S>(
    rw: &mut BufReader<S>,
    pending: &mut Vec<u8>,
    prev_positive: &mut i32,
    prev_negative: &mut i32,
) -> std::io::Result<i32>
where
    S: AsyncRead + Unpin,
{
    let mut b = data_take(rw, pending, 1).await?;
    let mut prev_is_negative = false;
    if b[0] == 0xFF {
        b = data_take(rw, pending, 1).await?;
        prev_is_negative = true;
    } else if b[0] == 0 {
        return Ok(NDX_DONE);
    }
    let unum: u32 = if b[0] == 0xFE {
        let two = data_take(rw, pending, 2).await?;
        if two[0] & 0x80 != 0 {
            let rest = data_take(rw, pending, 2).await?;
            let mut four = [0u8; 4];
            four[3] = two[0] & 0x7F;
            four[0] = two[1];
            four[1] = rest[0];
            four[2] = rest[1];
            u32::from_le_bytes(four)
        } else {
            ((two[0] as u32) << 8)
                .wrapping_add(two[1] as u32)
                .wrapping_add(*prev_positive as u32)
        }
    } else {
        (b[0] as u32).wrapping_add(*prev_positive as u32)
    };
    if unum > i32::MAX as u32 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "rsync: 非法文件索引",
        ));
    }
    let num = unum as i32;
    if prev_is_negative {
        *prev_negative = num;
        Ok(-num)
    } else {
        *prev_positive = num;
        Ok(num)
    }
}

/// 协议 30 的 NDX 编码（io.c `write_ndx` 移植 ✗ 差分位 + NDX_DONE 单字节 0）。
fn write_ndx(ndx: i32, prev_positive: &mut i32, prev_negative: &mut i32, out: &mut Vec<u8>) {
    if ndx == NDX_DONE {
        out.push(0);
        return;
    }
    let mut b = [0u8; 6];
    let mut cnt = 0usize;
    let diff;
    let mut num = ndx;
    if ndx >= 0 {
        diff = ndx - *prev_positive;
        *prev_positive = ndx;
    } else {
        b[cnt] = 0xFF;
        cnt += 1;
        num = -ndx;
        diff = num - *prev_negative;
        *prev_negative = num;
    }
    if (1..0xFE).contains(&diff) {
        b[cnt] = diff as u8;
        cnt += 1;
    } else if !(0..=0x7FFF).contains(&diff) {
        b[cnt] = 0xFE;
        b[cnt + 1] = ((num >> 24) as u8) | 0x80;
        b[cnt + 2] = num as u8;
        b[cnt + 3] = (num >> 8) as u8;
        b[cnt + 4] = (num >> 16) as u8;
        cnt += 5;
    } else {
        b[cnt] = 0xFE;
        b[cnt + 1] = (diff >> 8) as u8;
        b[cnt + 2] = diff as u8;
        cnt += 3;
    }
    out.extend_from_slice(&b[..cnt]);
}

/// 从解复用流读 4 字节 LE 有符号整数（io.c `read_int`）。
async fn data_int<S>(rw: &mut BufReader<S>, pending: &mut Vec<u8>) -> std::io::Result<i32>
where
    S: AsyncRead + Unpin,
{
    let b = data_take(rw, pending, 4).await?;
    Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// 从解复用流读 2 字节 LE 无符号（io.c `read_shortint`）。
async fn data_shortint<S>(rw: &mut BufReader<S>, pending: &mut Vec<u8>) -> std::io::Result<u16>
where
    S: AsyncRead + Unpin,
{
    let b = data_take(rw, pending, 2).await?;
    Ok((b[0] as u16) | ((b[1] as u16) << 8))
}

/// 从解复用流读 vstring（io.c `read_vstring`）。
async fn data_vstring<S>(rw: &mut BufReader<S>, pending: &mut Vec<u8>) -> std::io::Result<String>
where
    S: AsyncRead + Unpin,
{
    let first = data_take(rw, pending, 1).await?[0];
    let len = if first & 0x80 != 0 {
        ((first & 0x7F) as usize) * 0x100 + data_take(rw, pending, 1).await?[0] as usize
    } else {
        first as usize
    };
    if len == 0 {
        return Ok(String::new());
    }
    let bytes = data_take(rw, pending, len).await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// 从解复用流读 1 字节。
async fn data_byte<S>(rw: &mut BufReader<S>, pending: &mut Vec<u8>) -> std::io::Result<u8>
where
    S: AsyncRead + Unpin,
{
    Ok(data_take(rw, pending, 1).await?[0])
}

/// `io.c:int_byte_extra` 表（varint/varlong 首字节的高位前缀长度）。
fn int_byte_extra(ch: u8) -> usize {
    match ch {
        0x00..=0x7F => 0,
        0x80..=0xBF => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        0xF8..=0xFB => 5,
        _ => 6,
    }
}

/// varint 读（io.c `read_varint` 移植 ✗ push 收 flist 用）。
async fn data_varint<S>(rw: &mut BufReader<S>, pending: &mut Vec<u8>) -> std::io::Result<i32>
where
    S: AsyncRead + Unpin,
{
    let ch = data_byte(rw, pending).await?;
    let extra = int_byte_extra(ch);
    let mut b = [0u8; 5];
    if extra > 0 {
        let bytes = data_take(rw, pending, extra).await?;
        b[..extra].copy_from_slice(&bytes);
        let bit: u8 = 1u8 << (8 - extra);
        b[extra] = ch & (bit - 1);
    } else {
        b[0] = ch;
    }
    Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// varlong(min) 读（io.c `read_varlong` 移植）。
async fn data_varlong<S>(
    rw: &mut BufReader<S>,
    pending: &mut Vec<u8>,
    min: usize,
) -> std::io::Result<i64>
where
    S: AsyncRead + Unpin,
{
    let b2 = data_take(rw, pending, min).await?;
    let mut u = [0u8; 9];
    u[..min - 1].copy_from_slice(&b2[1..min]);
    let ctrl = b2[0];
    let extra = int_byte_extra(ctrl);
    if extra > 0 {
        let bytes = data_take(rw, pending, extra).await?;
        u[min - 1..min - 1 + extra].copy_from_slice(&bytes);
        let bit: u8 = 1u8 << (8 - extra);
        u[min - 1 + extra] = ctrl & (bit - 1);
    } else {
        u[min - 1] = ctrl;
    }
    let mut v = [0u8; 8];
    v.copy_from_slice(&u[..8]);
    Ok(i64::from_le_bytes(v))
}

/// flist 解析开关（决定条目里出现哪些可选字段 ✗ 官方 `preserve_*` 全局量同源）。
#[derive(Debug, Default, Clone, Copy)]
pub struct FlistOpts {
    /// `CF_VARINT_FLIST_FLAGS`（1<<7）→ xflags 为 varint。
    pub varint_flags: bool,
    /// `-c/--checksum` → 普通文件末附 16B 整文件 MD5。
    pub always_checksum: bool,
    /// `-o/--owner` → 条目含 uid（varint ✗ 名字随行仅增量递归时）。
    pub preserve_uid: bool,
    /// `-g/--group` → 条目含 gid。
    pub preserve_gid: bool,
    /// `-D/--devices` → 设备条目含 rdev（varint30 major + varint minor）。
    pub preserve_devices: bool,
    /// `-D/--specials` → 特殊文件条目含 rdev（协议 31 前）。
    pub preserve_specials: bool,
    /// `-U/--atimes` → 条目含 atime（varlong4 ✗ 目录不附）。
    pub preserve_atimes: bool,
}

/// 接收客户端发来的 flist（recv_file_entry 逐字段逆序 ✗ `lastname` 前缀压缩重建）。
///
/// 字段序（官方 `send_file_entry`/`recv_file_entry` 对称 ✗ r18 补齐 `-a` 所需）：
/// xflags → 名 → [hlink ndx] → 长 → [mtime] → [nsec] → [mode] → [atime] → [uid] →
/// [gid] → [rdev] → [symlink target] → [整文件校验和]。
pub async fn recv_file_list<S>(
    rw: &mut BufReader<S>,
    pending: &mut Vec<u8>,
    opts: FlistOpts,
) -> std::io::Result<Vec<FlatEntry>>
where
    S: AsyncRead + Unpin,
{
    let varint_flags = opts.varint_flags;
    let always_checksum = opts.always_checksum;
    let mut out = Vec::new();
    let mut lastname = String::new();
    let mut last_mode: u32 = 0;
    let mut last_mtime: i64 = 0;
    loop {
        let x: u32 = if varint_flags {
            data_varint(rw, pending).await? as u32
        } else {
            let b0 = data_byte(rw, pending).await? as u32;
            if b0 == 0 {
                0
            } else if b0 & 0x04 != 0 {
                b0 | ((data_byte(rw, pending).await? as u32) << 8)
            } else {
                b0
            }
        };
        if x == 0 {
            if varint_flags {
                let _io_error = data_varint(rw, pending).await?;
            }
            break;
        }
        let l1 = if x & 0x20 != 0 {
            data_byte(rw, pending).await? as usize
        } else {
            0
        };
        let l2 = if x & 0x40 != 0 {
            data_varint(rw, pending).await? as usize
        } else {
            data_byte(rw, pending).await? as usize
        };
        let suffix = data_take(rw, pending, l2).await?;
        let mut name_bytes = lastname
            .as_bytes()
            .get(..l1.min(lastname.len()))
            .unwrap_or_default()
            .to_vec();
        name_bytes.extend_from_slice(&suffix);
        let name = String::from_utf8_lossy(&name_bytes).into_owned();
        lastname = name.clone();
        // XMIT_HLINKED（`-H` 硬链接 ✗ `-a` 不含）→ 首个同 inode 条目的 ndx
        if x & (1 << 9) != 0 {
            let _hlink_ndx = data_varint(rw, pending).await?;
        }

        let size = data_varlong(rw, pending, 3).await?.max(0) as u64;
        let mtime = if x & 0x80 == 0 {
            data_varlong(rw, pending, 4).await?
        } else {
            last_mtime
        };
        last_mtime = mtime;
        if x & (1 << 13) != 0 {
            let _nsec = data_varint(rw, pending).await?;
        }
        let mode = if x & 0x02 == 0 {
            data_int(rw, pending).await? as u32
        } else {
            last_mode
        };
        last_mode = mode;
        let file_type = mode & 0o170000;
        let is_dir = file_type == 0o040000;
        // atime（`-U` ✗ 目录不附）
        if opts.preserve_atimes && !is_dir && x & (1 << 14) == 0 {
            let _atime = data_varlong(rw, pending, 4).await?;
        }
        // uid / gid（`-a` 含 `-o -g` ✗ 同值或首个条目外不重发）
        if opts.preserve_uid && x & (1 << 3) == 0 {
            let _uid = data_varint(rw, pending).await?;
            if x & (1 << 10) != 0 {
                let l = data_byte(rw, pending).await? as usize;
                let _name = data_take(rw, pending, l).await?;
            }
        }
        if opts.preserve_gid && x & (1 << 4) == 0 {
            let _gid = data_varint(rw, pending).await?;
            if x & (1 << 11) != 0 {
                let l = data_byte(rw, pending).await? as usize;
                let _name = data_take(rw, pending, l).await?;
            }
        }
        // rdev（设备/特殊文件 ✗ 协议 30 = varint major + varint minor）
        if (opts.preserve_devices && file_type == 0o020000)
            || (opts.preserve_specials && file_type == 0o010000)
        {
            let _major = data_varint(rw, pending).await?;
            let _minor = data_varint(rw, pending).await?;
        }
        // 符号链接 target（`-l` 时才有 ✗ 位置在 uid/gid/rdev **之后**）
        if file_type == 0o120000 {
            let l = data_varint(rw, pending).await?.max(0) as usize;
            let _target = data_take(rw, pending, l).await?;
        }
        // `-c`：普通文件追加整文件校验和（flist 末字段 ✗ 长度 = 协商 md5 = 16）
        let file_sum = if always_checksum && file_type == 0o100000 {
            Some(data_take(rw, pending, 16).await?.to_vec())
        } else {
            None
        };
        out.push(FlatEntry {
            name,
            is_dir,
            size,
            mtime,
            mode,
            fs_path: String::new(),
            file_sum,
        });
    }
    Ok(out)
}

/// 读走 uid/gid 名列表（官方 `recv_id_list`：flist 之后、传输之前 ✗ `-o`/`-g` 且非 numeric-ids）。
///
/// 形 = `[varint id][byte len][name]…` 终结：`xmit_id0_names` 时 id0 条目自带名字，否则裸 `varint 0`。
async fn skip_id_list<S>(
    rw: &mut BufReader<S>,
    pending: &mut Vec<u8>,
    xmit_id0_names: bool,
) -> std::io::Result<()>
where
    S: AsyncRead + Unpin,
{
    loop {
        let id = data_varint(rw, pending).await?;
        if id == 0 && !xmit_id0_names {
            return Ok(());
        }
        let len = data_byte(rw, pending).await? as usize;
        if len > 0 {
            let _name = data_take(rw, pending, len).await?;
        }
        if id == 0 {
            return Ok(());
        }
    }
}

/// 发送空 uid/gid 名列表（终结形随 `xmit_id0_names`：带 id0 名字则 varint(0)+byte(0)）。
async fn write_id_list<S>(rw: &mut BufReader<S>, xmit_id0_names: bool) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut buf = Vec::new();
    write_varint(0, &mut buf);
    if xmit_id0_names {
        buf.push(0);
    }
    // 出向已多路复用（与 flist/文件数据同流）→ 必须打 MSG_DATA 帧
    write_msg(rw, &buf).await
}

/// 文件内容 MD5（真机实证：rsync 整文件校验和 = `MD5(内容)`，**不含 seed** ✗ 见 golden r9）。
pub fn md5_digest(data: &[u8]) -> [u8; 16] {
    use md5::{Digest, Md5};
    let mut h = Md5::new();
    h.update(data);
    let out = h.finalize();
    let mut digest = [0u8; 16];
    digest.copy_from_slice(&out);
    digest
}

/// 块强校验和（真机实证 r5：`MD5(seed_le ‖ 块)` 前缀 `s2length` 字节 ✗ `get_checksum2`
/// 的 `proper_seed_order` 分支）。整文件校验和走 [`md5_digest`]（无 seed）✗ 两者不同。
fn md5_seeded(seed: u32, data: &[u8]) -> [u8; 16] {
    use md5::{Digest, Md5};
    let mut h = Md5::new();
    h.update(seed.to_le_bytes());
    h.update(data);
    let out = h.finalize();
    let mut digest = [0u8; 16];
    digest.copy_from_slice(&out);
    digest
}

/// 接收端块校验和（弱 `sum1` + 强 `sum2` 前缀 + 块长）。
#[derive(Debug, Clone)]
pub struct BlockSum {
    pub sum1: u32,
    pub sum2: Vec<u8>,
    pub len: u32,
}

/// `schar` 语义的单字节符号扩展（rsync `get_checksum1` 用 **signed char** ✗ 真机实证）。
#[inline]
fn signed_byte(b: u8) -> u32 {
    (b as i8) as i32 as u32
}

/// 弱滚动校验和（rsync `get_checksum1` 的 `CHAR_OFFSET=0` 形式，逐步累加等价于其展开式）。
/// 返回 `(s1, s2)`；公开值 = `(s1 & 0xffff) | (s2 << 16)`。
fn checksum1_signed(data: &[u8]) -> (u32, u32) {
    let mut s1: u32 = 0;
    let mut s2: u32 = 0;
    for &b in data {
        s1 = s1.wrapping_add(signed_byte(b));
        s2 = s2.wrapping_add(s1);
    }
    (s1, s2)
}

/// literal 段写出（每 ≤`CHUNK_SIZE` 一块 `int32(len)+data` ✗ 与 `simple_send_token` 同形）。
fn emit_literal(out: &mut Vec<u8>, data: &[u8]) {
    for chunk in data.chunks(CHUNK_SIZE) {
        out.extend_from_slice(&(chunk.len() as i32).to_le_bytes());
        out.extend_from_slice(chunk);
    }
}

/// 接收端块大小启发（sqrt 夹取 ✗ 与官方 `sum_sizes_sqroot` 同量级；具体值双方经 sum_head 对齐）。
fn block_size(n: usize) -> u32 {
    if n == 0 {
        return 0;
    }
    ((n as f64).sqrt() as u32).clamp(700, 32 * 1024)
}

/// 由本地 basis 生成块校验和（弱 sum1 int32 + 强 `MD5(seed‖块)` 前 `s2length` 字节）。
/// 返回 `(count, remainder, 序列化块字节)` ✗ 与官方 `write_sum_head` + `sums[]` 同形。
pub fn build_block_sums(
    basis: &[u8],
    blength: u32,
    seed: u32,
    s2length: usize,
) -> (i32, u32, Vec<u8>) {
    let b = blength as usize;
    if b == 0 || basis.is_empty() {
        return (0, 0, Vec::new());
    }
    let n = basis.len();
    let count = n.div_ceil(b);
    let remainder = (n % b) as u32;
    let mut out = Vec::with_capacity(count * (4 + s2length));
    for i in 0..count {
        let start = i * b;
        let end = (start + b).min(n);
        let chunk = &basis[start..end];
        let (s1, s2) = checksum1_signed(chunk);
        let weak = (s1 & 0xffff) | (s2 << 16);
        out.extend_from_slice(&(weak as i32).to_le_bytes());
        out.extend_from_slice(&md5_seeded(seed, chunk)[..s2length]);
    }
    (count as i32, remainder, out)
}

/// 应用 token 流重建文件（接收端 ✗ literal 段 + basis 块匹配）。
///
/// token 编码：`>0` = 后随 n 字节 literal；`<0` = 匹配 basis 块 `idx = -t-1`；`0` = 终结。
pub fn apply_tokens(
    tokens: &[u8],
    basis: &[u8],
    blength: u32,
    count: i32,
    remainder: u32,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let b = blength as usize;
    let mut i = 0usize;
    while i + 4 <= tokens.len() {
        let t = i32::from_le_bytes([tokens[i], tokens[i + 1], tokens[i + 2], tokens[i + 3]]);
        i += 4;
        if t == 0 {
            return Ok(out);
        }
        if t > 0 {
            let len = t as usize;
            if i + len > tokens.len() {
                return Err("literal 越界".to_string());
            }
            out.extend_from_slice(&tokens[i..i + len]);
            i += len;
        } else {
            let idx = (-t - 1) as usize;
            if b == 0 || idx >= count.max(0) as usize {
                return Err("匹配索引越界".to_string());
            }
            let start = idx * b;
            let len = if idx == count as usize - 1 && remainder != 0 {
                remainder as usize
            } else {
                b
            };
            if start + len > basis.len() {
                return Err("basis 越界".to_string());
            }
            out.extend_from_slice(&basis[start..start + len]);
        }
    }
    Err("token 流未终结".to_string())
}

/// delta token 流（flist 块校验和 vs 本文件内容）：滚动弱校验和命中 → 强校验和确认 →
/// 发匹配 token `-(idx+1)`；未命中区段作 literal 发送；末尾 `int32(0)` 终结。
///
/// 算法照 rsync `match.c:hash_search`（signed char 滚动 + 末尾短块 `end` 边界）。
pub fn build_delta_tokens(
    data: &[u8],
    blocks: &[BlockSum],
    blength: u32,
    seed: u32,
    s2length: usize,
) -> Vec<u8> {
    use std::collections::HashMap;
    let mut out = Vec::new();
    let len = data.len();
    let b = blength as usize;
    if blocks.is_empty() || b == 0 || len == 0 {
        emit_literal(&mut out, data);
        out.extend_from_slice(&0i32.to_le_bytes());
        return out;
    }
    // 弱校验和 → 候选块（先弱后强 = 官方同序）
    let mut index: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, blk) in blocks.iter().enumerate() {
        index.entry(blk.sum1).or_default().push(i);
    }
    let last_len = blocks[blocks.len() - 1].len as usize;
    let end = (len + 1).saturating_sub(last_len);
    let mut offset = 0usize;
    let mut last_match = 0usize;
    let mut k = b.min(len);
    let (mut s1, mut s2) = checksum1_signed(&data[..k]);
    loop {
        let weak = (s1 & 0xffff) | (s2 << 16);
        let mut hit: Option<usize> = None;
        if let Some(cands) = index.get(&weak) {
            for &i in cands {
                if blocks[i].len as usize != k || blocks[i].sum2.len() < s2length {
                    continue;
                }
                let strong = md5_seeded(seed, &data[offset..offset + k]);
                if strong[..s2length] == blocks[i].sum2[..s2length] {
                    hit = Some(i);
                    break;
                }
            }
        }
        if let Some(i) = hit {
            if offset > last_match {
                emit_literal(&mut out, &data[last_match..offset]);
            }
            out.extend_from_slice(&(-(i as i32 + 1)).to_le_bytes());
            offset += blocks[i].len as usize;
            last_match = offset;
            if offset >= len {
                break;
            }
            k = b.min(len - offset);
            let (a, c) = checksum1_signed(&data[offset..offset + k]);
            s1 = a;
            s2 = c;
            continue;
        }
        if offset + 1 >= end {
            break;
        }
        // 滚动：去首字节、加尾字节（signed char 语义 ✗ 与官方逐位同）
        let more = offset + k < len;
        s1 = s1.wrapping_sub(signed_byte(data[offset]));
        s2 = s2.wrapping_sub((k as u32).wrapping_mul(signed_byte(data[offset])));
        if more {
            s1 = s1.wrapping_add(signed_byte(data[offset + k]));
            s2 = s2.wrapping_add(s1);
        } else {
            k -= 1;
        }
        offset += 1;
    }
    if len > last_match {
        emit_literal(&mut out, &data[last_match..len]);
    }
    out.extend_from_slice(&0i32.to_le_bytes());
    out
}

/// 写一条 mux MSG_DATA 帧（大 payload 自动分片 ≤ 64KB/帧）。
async fn write_msg<S>(rw: &mut BufReader<S>, payload: &[u8]) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut buf = Vec::with_capacity(payload.len() + 4);
    for chunk in payload.chunks(64 * 1024) {
        buf.extend_from_slice(&mux_frame(chunk));
    }
    write_raw(rw, &buf).await
}

// ─────────────────────────── L1 状态机 ───────────────────────────

/// rsync 连接的数据后端（下载取数 / 上传落库 / `--delete` 删条目 ✗ bin 装配 domain 链）。
#[async_trait::async_trait]
pub trait RsyncBackend: Send + Sync {
    /// 枚举请求路径下的条目（`.` 必须首条 ✗ 下载面）。
    async fn list(&self, req: ListRequest) -> Result<Vec<FlatEntry>, String>;
    /// 按命名空间路径读文件内容（下载 / 收端 delta basis / `--delete` 存在性）。
    async fn read(&self, path: &str) -> Result<Vec<u8>, String>;
    /// 按命名空间路径写入文件（push ✗ `mtime` = 源端秒级时间，供快跳比对）。
    async fn write(&self, path: String, data: Vec<u8>, mtime: i64) -> Result<(), String>;
    /// 查目标条目的 `(size, 源 mtime)`；不存在 → `None`（push 快跳用）。
    async fn stat(&self, path: &str) -> Result<Option<(u64, i64)>, String>;
    /// 删除命名空间路径（`--delete` 镜像 ✗ 目录含后代）。
    async fn delete(&self, paths: Vec<String>) -> Result<(), String>;
    /// 创建目录（push 空目录 ✗ 已存在视为成功）。
    async fn mkdir(&self, path: &str) -> Result<(), String>;
}

/// 处理一条 rsync daemon 连接（协议 30 ✗ 双向：下载/上传/增量/认证）。
pub async fn handle_conn<S, B>(
    stream: S,
    module: &str,
    auth: AuthConfig,
    backend: &B,
) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
    B: RsyncBackend,
{
    let mut rw = BufReader::new(stream);

    // ① 服务端 banner 与客户端版本行（双向 greeting ✗ 各写各的）
    write_raw(&mut rw, PROTOCOL_LINE.as_bytes()).await?;
    let client_line = read_raw_line(&mut rw).await?;
    if client_line.is_empty() {
        return Ok(());
    }
    match parse_version(&client_line) {
        Some(v) if v >= 30 => {}
        _ => {
            write_raw(&mut rw, b"@RSYNCD: ERROR unsupported protocol version\n").await?;
            return Ok(());
        }
    }

    // ② 模块列表 / 模块选择
    let request = read_raw_line(&mut rw).await?;
    if request.is_empty() {
        return Ok(());
    }
    let text = request.trim_end_matches(['\n', '\r']).to_string();
    if text.is_empty() {
        let line = format!("{:<15}\t\n", module);
        write_raw(&mut rw, line.as_bytes()).await?;
        write_raw(&mut rw, b"@RSYNCD: EXIT\n").await?;
        return Ok(());
    }
    if text != module {
        // 官方 P04 形（单行即关 ✗ 真 CLI 逐字同）
        let line = format!("@ERROR: Unknown module '{}'\n", text);
        write_raw(&mut rw, line.as_bytes()).await?;
        return Ok(());
    }
    // 认证（secrets challenge-response ✗ 成功才 OK；失败按官方单行 @ERROR）
    let mut writable = auth.writable;
    if auth.required() {
        let client_digests = parse_auth_list(&client_line);
        let digest = pick_digest(&client_digests);
        let challenge = gen_challenge();
        let line = format!("@RSYNCD: AUTHREQD {challenge}\n");
        write_raw(&mut rw, line.as_bytes()).await?;
        let resp = read_raw_line(&mut rw).await?;
        let resp = resp.trim_end_matches(['\n', '\r']);
        let verified = match resp.split_once(' ') {
            Some((user, response)) => auth.verify(user, response, &challenge, digest),
            None => None,
        };
        match verified {
            Some(w) => writable = w,
            None => {
                tracing::warn!("rsync：模块 {module} 认证失败");
                let line = format!("@ERROR: auth failed on module {module}\n");
                let _ = write_raw(&mut rw, line.as_bytes()).await;
                return Ok(());
            }
        }
    }
    write_raw(&mut rw, b"@RSYNCD: OK\n").await?;

    // ③ args（NUL 分隔、空段终结）
    let segs = read_arg_segments(&mut rw).await?;
    let args = parse_args(&segs);

    // ④ setup_protocol：compat_flags → 协商字符串 → checksum_seed
    let compat = compute_compat(&args.client_info);
    let negotiated = compat & CF_VARINT_FLIST_FLAGS != 0;
    let mut head = Vec::new();
    write_varint(compat as i32, &mut head);
    if negotiated {
        write_vstring(CHECKSUM_LIST, &mut head);
        if args.compress {
            write_vstring(COMPRESS_LIST, &mut head);
        }
    }
    write_raw(&mut rw, &head).await?;
    if negotiated {
        // 客户端 checksum 清单（回读以推进流；选择已由双方各自计算）
        let _client_sums = read_vstring(&mut rw).await?;
        if args.compress {
            let _client_compress = read_vstring(&mut rw).await?;
        }
    }
    let seed = make_seed();
    write_raw(&mut rw, &seed.to_le_bytes()).await?;

    // 只读模块拒收 push（官方 do_server_recv：multiplexed MSG_ERROR = tag 10）
    if !args.is_sender && !writable {
        tracing::warn!("rsync：模块只读，拒绝 push");
        write_raw(
            &mut rw,
            &mux_frame_tagged(b"ERROR: module is read only\n", 3),
        )
        .await?;
        return Ok(());
    }

    // 解复用输入缓冲（sender/receiver 两径共用）
    let mut pending: Vec<u8> = Vec::new();
    if args.is_sender {
        // ⑤ filter list（官方 recv_filter_list：int len + 规则，0 终结）
        loop {
            let b = data_take(&mut rw, &mut pending, 4).await?;
            let len = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            if len == 0 {
                break;
            }
            if !(0..=64 * 1024).contains(&len) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "rsync: 非法 filter 规则长度",
                ));
            }
            let _rule = data_take(&mut rw, &mut pending, len as usize).await?;
        }

        // ⑥ flist：收集 → 按 rsync 序排序（ndx 对齐）→ 编码 → 发送
        let req = ListRequest {
            recursive: args.recursive,
            path: module_path(&args.paths, module),
        };
        let mut entries = match backend.list(req).await {
            Ok(entries) => entries,
            Err(err) => {
                tracing::warn!(error = %err, "rsync：列举失败，回空清单");
                vec![FlatEntry::dir(".", now_unix())]
            }
        };
        sort_flist(&mut entries);
        let flist = encode_flist(&entries, negotiated);
        let total_size: u64 = entries.iter().filter(|e| !e.is_dir).map(|e| e.size).sum();
        write_msg(&mut rw, &flist).await?;
        // `-o`/`-g` 且非 `--numeric-ids`：**本端为发送端** → flist 后紧跟 uid/gid 名列表
        // （官方 `send_id_lists` ✗ 空表 = 不做 id 映射，值与名都不传）
        if !args.numeric_ids {
            let xmit_id0 = compat & CF_ID0_NAMES != 0;
            if args.preserve_uid {
                write_id_list(&mut rw, xmit_id0).await?;
            }
            if args.preserve_gid {
                write_id_list(&mut rw, xmit_id0).await?;
            }
        }

        // ⑦ send_files：读接收端请求（ndx + iflags[+basis/xname] + sum_head[+块校验和]）→
        //    回显 + 全 literal 数据 + MD5；NDX_DONE 走相位机（3 入 → 2 出 + 终结）。
        let mut phase = 0u32;
        let mut rp = (-1i32, 1i32); // read_ndx 差分态
        let mut wp = (-1i32, 1i32); // write_ndx 差分态
        loop {
            let ndx = read_ndx(&mut rw, &mut pending, &mut rp.0, &mut rp.1).await?;
            if ndx == NDX_DONE {
                phase += 1;
                if phase > 2 {
                    break;
                }
                let mut out = Vec::new();
                write_ndx(NDX_DONE, &mut wp.0, &mut wp.1, &mut out);
                write_msg(&mut rw, &out).await?;
                continue;
            }
            if ndx < 0 {
                tracing::warn!(ndx = ndx, "rsync：未预期的负索引，结束会话");
                break;
            }

            // 请求属性
            let iflags = data_shortint(&mut rw, &mut pending).await?;
            let mut basis_type = 0u8;
            if iflags & ITEM_BASIS_TYPE_FOLLOWS != 0 {
                basis_type = data_take(&mut rw, &mut pending, 1).await?[0];
            }
            let xname = if iflags & ITEM_XNAME_FOLLOWS != 0 {
                data_vstring(&mut rw, &mut pending).await?
            } else {
                String::new()
            };

            // 仅 ITEM_TRANSFER 才读接收端 sum_head + 块校验和（官方 send_files 同分支顺序）
            let transferring = iflags & ITEM_TRANSFER != 0;
            let (count, blength, s2length, remainder, blocks) = if transferring {
                let c = data_int(&mut rw, &mut pending).await?;
                let bl = data_int(&mut rw, &mut pending).await?;
                let s2 = data_int(&mut rw, &mut pending).await?;
                let rem = data_int(&mut rw, &mut pending).await?;
                if !(0..=16 * 1024 * 1024).contains(&c) || !(0..=64).contains(&s2) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "rsync: 非法 sum_head",
                    ));
                }
                // 块校验和（弱 sum1 int32 + 强 sum2 s2length ✗ 真 delta 匹配用）
                let mut blocks = Vec::with_capacity(c as usize);
                for i in 0..c {
                    let sum1 = data_int(&mut rw, &mut pending).await? as u32;
                    let sum2 = data_take(&mut rw, &mut pending, s2 as usize).await?;
                    let blen = if i == c - 1 && rem != 0 {
                        rem as u32
                    } else {
                        bl as u32
                    };
                    blocks.push(BlockSum {
                        sum1,
                        sum2,
                        len: blen,
                    });
                }
                (c, bl, s2, rem, blocks)
            } else {
                (0, 0, 0, 0, Vec::new())
            };

            // 回显 ndx+attrs（+ sum_head 回显）与数据
            let mut out = Vec::new();
            write_ndx(ndx, &mut wp.0, &mut wp.1, &mut out);
            out.extend_from_slice(&iflags.to_le_bytes());
            if iflags & ITEM_BASIS_TYPE_FOLLOWS != 0 {
                out.push(basis_type);
            }
            if iflags & ITEM_XNAME_FOLLOWS != 0 {
                write_vstring(&xname, &mut out);
            }

            if transferring {
                out.extend_from_slice(&count.to_le_bytes());
                out.extend_from_slice(&blength.to_le_bytes());
                out.extend_from_slice(&s2length.to_le_bytes());
                out.extend_from_slice(&remainder.to_le_bytes());

                let entry = entries.get(ndx as usize);
                let fs_path = entry.map(|e| e.fs_path.clone()).unwrap_or_default();
                match backend.read(&fs_path).await {
                    Ok(data) => {
                        // count>0 = 有 basis → 真 delta（弱 sum1 滚动 + 强 sum2 前缀匹配）；
                        // 否则整文件 literal（无 basis 的常规路径）
                        if blocks.is_empty() || blength <= 0 {
                            emit_literal(&mut out, &data);
                            out.extend_from_slice(&0i32.to_le_bytes()); // token 终结
                        } else {
                            let tokens = build_delta_tokens(
                                &data,
                                &blocks,
                                blength as u32,
                                seed,
                                s2length as usize,
                            );
                            out.extend_from_slice(&tokens);
                        }
                        out.extend_from_slice(&md5_digest(&data)); // 文件校验和（无 seed ✗ 真机实证）
                    }
                    Err(err) => {
                        tracing::warn!(path = %fs_path, error = %err, "rsync：文件读取失败，回 MSG_NO_SEND");
                        let mut msg = Vec::new();
                        msg.extend_from_slice(&(ndx as i32).to_le_bytes());
                        write_msg(&mut rw, &mux_frame_tagged(&msg, MSG_NO_SEND)).await?;
                        continue;
                    }
                }
            }
            write_msg(&mut rw, &out).await?;
        }
        let mut done = Vec::new();
        write_ndx(NDX_DONE, &mut wp.0, &mut wp.1, &mut done);
        write_msg(&mut rw, &done).await?;

        let mut stats = Vec::new();
        write_varlong(3, 0, &mut stats); // total_read
        write_varlong(3, flist.len() as i64, &mut stats); // total_written（近似）
        write_varlong(3, total_size as i64, &mut stats);
        write_varlong(3, 1, &mut stats); // flist_buildtime
        write_varlong(3, 0, &mut stats); // flist_xfertime
        write_msg(&mut rw, &stats).await?;

        // 最后一个 NDX_DONE 问候（官方 read_final_goodbye 协议 30 分支）
        let _ = read_ndx(&mut rw, &mut pending, &mut rp.0, &mut rp.1).await?;
        Ok(())
    } else {
        // ── push：客户端为 sender、本端为接收端 ──
        let base = module_path(&args.paths, module);
        // `--delete*` → 客户端先发 filter list（receiver_wants_list=true）；规则用于**保护**不被删
        let mut filter_rules: Vec<FilterRule> = Vec::new();
        if args.delete {
            loop {
                let b = data_take(&mut rw, &mut pending, 4).await?;
                let len = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                if len == 0 {
                    break;
                }
                if !(0..=64 * 1024).contains(&len) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "rsync: 非法 filter 规则长度（push）",
                    ));
                }
                let rule = data_take(&mut rw, &mut pending, len as usize).await?;
                let text = String::from_utf8_lossy(&rule);
                tracing::debug!(rule = %text, "FILTER-RULE");
                match parse_rule(&text) {
                    Some(r) => filter_rules.push(r),
                    None => tracing::debug!(rule = %text, "rsync: 跳过不支持的 filter 规则"),
                }
            }
            tracing::debug!(rules = filter_rules.len(), "rsync: 已解析 filter 规则");
        }
        let mut entries = recv_file_list(
            &mut rw,
            &mut pending,
            FlistOpts {
                varint_flags: negotiated,
                always_checksum: args.checksum,
                preserve_uid: args.preserve_uid,
                preserve_gid: args.preserve_gid,
                preserve_devices: args.preserve_devices,
                preserve_specials: args.preserve_specials,
                preserve_atimes: args.preserve_atimes,
            },
        )
        .await?;
        // 诊断用：VFILES_RSYNC_DUMP_FLIST=1 时逐条打印收到的 flist（协议对齐排障）
        if std::env::var("VFILES_RSYNC_DUMP_FLIST").is_ok() {
            for (i, e) in entries.iter().enumerate() {
                tracing::warn!(
                    i,
                    name = %e.name,
                    is_dir = e.is_dir,
                    size = e.size,
                    mtime = e.mtime,
                    mode = format!("{:o}", e.mode),
                    "FLIST"
                );
            }
        }
        // `-o`/`-g` 且非 `--numeric-ids`：**本端为接收端** → flist 后紧跟 uid/gid 名列表（读走）
        if !args.numeric_ids {
            let xmit_id0 = compat & CF_ID0_NAMES != 0;
            if args.preserve_uid {
                skip_id_list(&mut rw, &mut pending, xmit_id0).await?;
            }
            if args.preserve_gid {
                skip_id_list(&mut rw, &mut pending, xmit_id0).await?;
            }
        }
        sort_flist(&mut entries);
        let mut rp = (-1i32, 1i32); // read_ndx 差分态
        let mut wp = (-1i32, 1i32); // write_ndx 差分态
        let mut transferred = 0usize;
        let to = std::time::Duration::from_secs(15);
        for (i, e) in entries.iter().enumerate() {
            if e.name == "." {
                continue;
            }
            let full = if base.is_empty() {
                e.name.clone()
            } else {
                format!("{base}/{}", e.name)
            };
            // 目录：显式创建（空目录不落地 = 此前债）；非普通文件（符号链接等）跳过
            if e.is_dir {
                if let Err(err) = backend.mkdir(&full).await {
                    tracing::warn!(path = %full, error = %err, "rsync push：建目录失败");
                }
                continue;
            }
            if (e.mode & 0o170000) != 0o100000 {
                continue;
            }
            // 快跳（官方 generator `unchanged_file` 语义）：
            // 默认 = size + 源 mtime 双等；`-c` = 整文件校验和；`-I` 全关。
            let meta = backend.stat(&full).await.unwrap_or(None);
            if !args.ignore_times
                && !args.checksum
                && let Some((dsize, dmtime)) = meta
                && dsize == e.size
                && (args.size_only || (dmtime - e.mtime).abs() <= args.modify_window)
            {
                tracing::debug!(path = %full, "rsync：size+mtime 一致，跳过");
                continue;
            }
            // 取本地现有内容作 basis（存在 → 发块校验和请求真 delta；否则整文件）
            let basis = backend.read(&full).await.unwrap_or_default();
            // `-c/--checksum`：整文件 MD5 一致 → 完全跳过（不请求 = 不传）
            if let Some(sum) = &e.file_sum
                && !basis.is_empty()
                && md5_digest(&basis).as_slice() == sum.as_slice()
            {
                tracing::debug!(path = %full, "rsync -c：校验和一致，跳过");
                continue;
            }
            let s2len: usize = 16;
            let (count, blength, remainder, block_bytes) = if basis.is_empty() {
                (0i32, 0i32, 0i32, Vec::new())
            } else {
                let bl = block_size(basis.len());
                let (c, rem, bytes) = build_block_sums(&basis, bl, seed, s2len);
                (c, bl as i32, rem as i32, bytes)
            };
            let mut req = Vec::new();
            write_ndx(i as i32, &mut wp.0, &mut wp.1, &mut req);
            req.extend_from_slice(&(ITEM_TRANSFER | ITEM_IS_NEW).to_le_bytes());
            req.extend_from_slice(&count.to_le_bytes());
            req.extend_from_slice(&blength.to_le_bytes());
            req.extend_from_slice(&(s2len as i32).to_le_bytes());
            req.extend_from_slice(&remainder.to_le_bytes());
            req.extend_from_slice(&block_bytes);
            write_msg(&mut rw, &req).await?;

            // 应答：ndx 回显 + iflags + sum_head 回显
            let ndx = match tokio::time::timeout(
                to,
                read_ndx(&mut rw, &mut pending, &mut rp.0, &mut rp.1),
            )
            .await
            {
                Ok(Ok(v)) => v,
                _ => break,
            };
            if ndx != i as i32 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("rsync push: 索引错位 {ndx} != {i}"),
                ));
            }
            let _iflags = data_shortint(&mut rw, &mut pending).await?;
            for _ in 0..4 {
                let _ = data_int(&mut rw, &mut pending).await?;
            }
            // 收 token 流（原样缓冲）→ `apply_tokens` 重建（literal + basis 块匹配）
            let mut token_buf = Vec::new();
            loop {
                let t = data_int(&mut rw, &mut pending).await?;
                token_buf.extend_from_slice(&t.to_le_bytes());
                if t == 0 {
                    break;
                }
                if t > 0 {
                    let chunk = data_take(&mut rw, &mut pending, t as usize).await?;
                    token_buf.extend_from_slice(&chunk);
                }
            }
            let data = apply_tokens(
                &token_buf,
                &basis,
                blength.max(0) as u32,
                count,
                remainder.max(0) as u32,
            )
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            // 文件校验和（协商 md5 = 16B；读走以推进流）
            let _file_sum = data_take(&mut rw, &mut pending, 16).await?;
            match backend.write(full.clone(), data, e.mtime).await {
                Ok(()) => transferred += 1,
                Err(err) => tracing::warn!(path = %full, error = %err, "rsync push：写入失败"),
            }
        }
        // 相位收尾（客户端 sender：2 答 + 终结；本端 4 出 / 3 入 ✗ 超时防挂）
        for round in 0..4 {
            let mut d = Vec::new();
            write_ndx(NDX_DONE, &mut wp.0, &mut wp.1, &mut d);
            write_msg(&mut rw, &d).await?;
            if round < 3
                && tokio::time::timeout(to, read_ndx(&mut rw, &mut pending, &mut rp.0, &mut rp.1))
                    .await
                    .is_err()
            {
                break;
            }
        }
        // `--delete`：删目标端源端没有的条目（镜像 ✗ 递归才有意义；目录删含后代）
        if args.delete {
            if !args.recursive {
                tracing::warn!("rsync：--delete 需配合 -r（本次跳过删除）");
            } else {
                let src_names: std::collections::HashSet<&str> =
                    entries.iter().map(|e| e.name.as_str()).collect();
                match backend
                    .list(ListRequest {
                        recursive: true,
                        path: base.clone(),
                    })
                    .await
                {
                    Ok(dest) => {
                        // 保护：命中 exclude 的条目（含其子树）不删；`--delete-excluded` 时全删
                        let mut protected_dirs: std::collections::HashSet<String> =
                            std::collections::HashSet::new();
                        let mut extras: Vec<String> = Vec::new();
                        for d in dest.iter() {
                            if d.name == "." {
                                continue;
                            }
                            let mut anc_protected = false;
                            let mut prefix = String::new();
                            for comp in d.name.split('/') {
                                if !prefix.is_empty() {
                                    prefix.push('/');
                                }
                                prefix.push_str(comp);
                                if prefix != d.name && protected_dirs.contains(&prefix) {
                                    anc_protected = true;
                                    break;
                                }
                            }
                            let protected = !args.delete_excluded
                                && (anc_protected || is_excluded(&filter_rules, &d.name, d.is_dir));
                            if protected {
                                if d.is_dir {
                                    protected_dirs.insert(d.name.clone());
                                }
                                continue;
                            }
                            if !src_names.contains(d.name.as_str()) && !d.fs_path.is_empty() {
                                extras.push(d.fs_path.clone());
                            }
                        }
                        if !extras.is_empty() {
                            let n = extras.len();
                            match backend.delete(extras).await {
                                Ok(()) => tracing::info!(removed = n, "rsync --delete 完成"),
                                Err(e) => tracing::warn!(error = %e, "rsync --delete 失败"),
                            }
                        }
                    }
                    Err(e) => tracing::warn!(error = %e, "rsync --delete 列举目标失败"),
                }
            }
        }
        tracing::info!(files = transferred, delete = args.delete, "rsync push 完成");
        Ok(())
    }
}

/// 带指定 msgcode 的 mux 帧（`[len3, MPLEX_BASE+code] + payload`）。
fn mux_frame_tagged(payload: &[u8], code: u8) -> Vec<u8> {
    let len = payload.len();
    let mut out = Vec::with_capacity(len + 4);
    out.push((len & 0xFF) as u8);
    out.push(((len >> 8) & 0xFF) as u8);
    out.push(((len >> 16) & 0xFF) as u8);
    out.push(MPLEX_BASE + code);
    out.extend_from_slice(payload);
    out
}

/// 读 NUL 分隔 args 直到空段（双 NUL 尾）。
async fn read_arg_segments<S>(rw: &mut BufReader<S>) -> std::io::Result<Vec<String>>
where
    S: AsyncRead + Unpin,
{
    let mut segs = Vec::new();
    loop {
        let mut seg = Vec::new();
        let n = rw.read_until(b'\0', &mut seg).await?;
        if n == 0 || seg.len() == 1 {
            break; // EOF 或空段（第二个 NUL）
        }
        seg.pop();
        segs.push(String::from_utf8_lossy(&seg).into_owned());
    }
    Ok(segs)
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ─────────────────────────── L2 数据枚举 ───────────────────────────

/// domain 树 → `FlatEntry` 流（bin 使用 ✗ L1 单测不走此路）。
///
/// 顺序 = 深度优先前序：`.` → 各子项（目录紧随其后递归）✗ 嵌套名 = 全相对路径
/// （与官方非增量递归一致 ✗ 我们不置 CF_INC_RECURSE）。
pub async fn collect_flat(
    repo: &(dyn vfiles_domain::EntryRepo + Send + Sync),
    namespace: &NamespaceId,
    req: &ListRequest,
) -> Result<Vec<FlatEntry>, String> {
    use vfiles_domain::types::NormalizedPath;
    let base = NormalizedPath::new(&req.path).map_err(|e| format!("非法路径: {e}"))?;
    let base_entry = if req.path.is_empty() {
        None
    } else {
        match repo.find_by_path(namespace, &base).await {
            Ok(Some(entry)) => Some(entry),
            Ok(None) => return Err(format!("路径不存在: {}", req.path)),
            Err(e) => return Err(e.to_string()),
        }
    };
    // 单文件请求（`rsync rsync://host/module/file`）：flist 仅该文件、name = basename、无 `.`
    if let Some(entry) = &base_entry
        && !matches!(entry.entry_type, EntryKind::Directory)
    {
        // 大小取自父目录的批量 meta（EntryRepo 无单条 meta 接口）
        let full = base.as_str();
        let (parent_str, _) = full.rsplit_once('/').unwrap_or(("", full));
        let size = match NormalizedPath::new(parent_str) {
            Ok(parent) => repo
                .children_with_meta(namespace, &parent)
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|c| c.entry.path_norm.as_str() == full)
                .and_then(|c| c.size_bytes)
                .unwrap_or(0),
            Err(_) => 0,
        };
        let sum_mtime = repo
            .children_with_meta(
                namespace,
                &NormalizedPath::new(base.as_str().rsplit_once('/').map(|(p, _)| p).unwrap_or(""))
                    .map_err(|e| format!("非法父路径: {e}"))?,
            )
            .await
            .ok()
            .and_then(|v| {
                v.into_iter()
                    .find(|c| c.entry.path_norm.as_str() == base.as_str())
                    .and_then(|c| c.source_mtime)
            });
        return Ok(vec![
            FlatEntry::file(
                entry.name.clone(),
                size,
                sum_mtime.unwrap_or_else(|| entry.created_at.unix_timestamp()),
            )
            .with_fs_path(entry.path_norm.as_str().to_string()),
        ]);
    }
    let base_mtime = base_entry
        .as_ref()
        .map(|e| e.created_at.unix_timestamp())
        .unwrap_or_else(now_unix);
    let mut out = vec![FlatEntry::dir(".", base_mtime).with_fs_path(base.as_str().to_string())];
    walk(repo, namespace, &base, "", req.recursive, &mut out).await?;
    Ok(out)
}

async fn walk(
    repo: &(dyn vfiles_domain::EntryRepo + Send + Sync),
    namespace: &NamespaceId,
    path: &vfiles_domain::types::NormalizedPath,
    prefix: &str,
    recursive: bool,
    out: &mut Vec<FlatEntry>,
) -> Result<(), String> {
    let children = repo
        .children_with_meta(namespace, path)
        .await
        .map_err(|e| e.to_string())?;
    for m in children {
        let is_dir = matches!(m.entry.entry_type, EntryKind::Directory);
        let name = if prefix.is_empty() {
            m.entry.name.clone()
        } else {
            format!("{prefix}/{}", m.entry.name)
        };
        let fs_path = m.entry.path_norm.as_str().to_string();
        let mtime = m
            .source_mtime
            .unwrap_or_else(|| m.entry.created_at.unix_timestamp());
        if is_dir {
            out.push(FlatEntry::dir(name.clone(), mtime).with_fs_path(fs_path));
            if recursive {
                Box::pin(walk(
                    repo,
                    namespace,
                    &m.entry.path_norm,
                    &name,
                    recursive,
                    out,
                ))
                .await?;
            }
        } else {
            out.push(FlatEntry::file(name, m.size_bytes.unwrap_or(0), mtime).with_fs_path(fs_path));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试后端（内存文件表 ✗ 零 domain 依赖）。
    struct FakeBackend {
        entries: Vec<FlatEntry>,
        files: std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
        mtimes: std::sync::Mutex<std::collections::HashMap<String, i64>>,
    }

    impl FakeBackend {
        fn new(entries: Vec<FlatEntry>, files: Vec<(String, Vec<u8>)>) -> Self {
            Self {
                entries,
                files: std::sync::Mutex::new(files.into_iter().collect()),
                mtimes: std::sync::Mutex::new(std::collections::HashMap::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl RsyncBackend for FakeBackend {
        async fn list(&self, _req: ListRequest) -> Result<Vec<FlatEntry>, String> {
            Ok(self.entries.clone())
        }
        async fn read(&self, path: &str) -> Result<Vec<u8>, String> {
            self.files
                .lock()
                .unwrap()
                .get(path)
                .cloned()
                .ok_or_else(|| "not found".to_string())
        }
        async fn write(&self, path: String, data: Vec<u8>, mtime: i64) -> Result<(), String> {
            let mut f = self.files.lock().unwrap();
            f.insert(path.clone(), data);
            self.mtimes.lock().unwrap().insert(path, mtime);
            Ok(())
        }
        async fn stat(&self, path: &str) -> Result<Option<(u64, i64)>, String> {
            let f = self.files.lock().unwrap();
            Ok(f.get(path).map(|d| {
                (
                    d.len() as u64,
                    *self.mtimes.lock().unwrap().get(path).unwrap_or(&0),
                )
            }))
        }
        async fn delete(&self, _paths: Vec<String>) -> Result<(), String> {
            Ok(())
        }
        async fn mkdir(&self, path: &str) -> Result<(), String> {
            self.files
                .lock()
                .unwrap()
                .entry(format!("{path}/"))
                .or_default();
            Ok(())
        }
    }
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn varint_matches_official_encoding() {
        let mut o = Vec::new();
        write_varint(0x19, &mut o);
        assert_eq!(o, vec![0x19], "0x19 单字节");
        o.clear();
        write_varint(0x9A, &mut o);
        assert_eq!(o, vec![0x80, 0x9A], "154 → 80 9A（真机转录同）");
        o.clear();
        write_varint(0x98, &mut o);
        assert_eq!(o, vec![0x80, 0x98], "152 → 80 98");
        o.clear();
        write_varint(510, &mut o);
        assert_eq!(o, vec![0x81, 0xFE], "compat 0x1FE → 81 FE（真机转录同）");
    }

    #[test]
    fn varlong_matches_official_encoding() {
        let mut o = Vec::new();
        write_varlong(3, 4096, &mut o);
        assert_eq!(o, vec![0x00, 0x00, 0x10], "4096 → 00 00 10");
        o.clear();
        write_varlong(3, 6, &mut o);
        assert_eq!(o, vec![0x00, 0x06, 0x00], "6 → 00 06 00");
        o.clear();
        write_varlong(4, 0x6AB3A68C, &mut o);
        assert_eq!(o, vec![0x6A, 0x8C, 0xA6, 0xB3], "epoch → 6a 8c a6 b3");
        // 8MB 以上需要 extra 字节（旧实装 assert 之地）
        o.clear();
        write_varlong(3, 0x1234567, &mut o);
        assert_eq!(o.len(), 4, "大值扩展为 4 字节");
    }

    #[test]
    fn vstring_shape() {
        let mut o = Vec::new();
        write_vstring("none", &mut o);
        assert_eq!(o, b"\x04none");
        o.clear();
        write_vstring(CHECKSUM_LIST, &mut o);
        assert_eq!(o[0], CHECKSUM_LIST.len() as u8, "md5 → 长度前缀 3");
        assert_eq!(&o[1..], CHECKSUM_LIST.as_bytes());
    }

    /// 真机转录 golden（protocol 30 · 非递归 depth1 · varint flags）：
    /// `.`(4096, mode 40775) → `sub`(同 mode/mtime) → `a.txt`(size6, 同 mtime)
    #[test]
    fn encode_flist_matches_reference_transcript() {
        let mtime = 0x6AB3A68C_i64;
        let entries = vec![
            FlatEntry {
                name: ".".into(),
                is_dir: true,
                size: 4096,
                mtime,
                mode: 0o40775,
                fs_path: String::new(),
                file_sum: None,
            },
            FlatEntry {
                name: "sub".into(),
                is_dir: true,
                size: 4096,
                mtime,
                mode: 0o40775,
                fs_path: String::new(),
                file_sum: None,
            },
            FlatEntry {
                name: "a.txt".into(),
                is_dir: false,
                size: 6,
                mtime,
                mode: 0o100664,
                fs_path: String::new(),
                file_sum: None,
            },
        ];
        let body = encode_flist(&entries, true);
        let expected: Vec<u8> = vec![
            0x19, 0x01, 0x2e, 0x00, 0x00, 0x10, 0x6A, 0x8C, 0xA6, 0xB3, 0xFD, 0x41, 0x00, 0x00,
            0x80, 0x9A, 0x03, 0x73, 0x75, 0x62, 0x00, 0x00, 0x10, //
            0x80, 0x98, 0x05, 0x61, 0x2E, 0x74, 0x78, 0x74, 0x00, 0x06, 0x00, 0xB4, 0x81, 0x00,
            0x00, //
            0x00, 0x00,
        ];
        assert_eq!(body, expected, "flist 与官方 daemon 逐字节同");
    }

    /// 真机转录 golden（protocol 30 · 递归 · 嵌套名 = 全相对路径）。
    /// 官方在同一 MSG_DATA 内追加增量 flist；我们单 flist 全量（不置 INC_RECURSE）＝
    /// 同一条目的字节形必须一致。
    #[test]
    fn encode_flist_nested_name_shape() {
        let mtime = 0x6AB3A68C_i64;
        let entries = vec![FlatEntry {
            name: "sub/b.txt".into(),
            is_dir: false,
            size: 8,
            mtime,
            mode: 0o100664,
            fs_path: String::new(),
            file_sum: None,
        }];
        let body = encode_flist(&entries, true);
        // xflags = SAME_UID|SAME_GID（首条无 SAME_MODE/TIME）= 0x18 → varint 单字节 0x18
        assert_eq!(body[0], 0x18);
        assert_eq!(body[1], 9, "l2 = len('sub/b.txt')");
        assert_eq!(&body[2..11], b"sub/b.txt");
        assert_eq!(&body[11..14], &[0x00, 0x08, 0x00], "size 8 varlong3");
        assert_eq!(
            &body[14..18],
            &[0x6A, 0x8C, 0xA6, 0xB3],
            "mtime varlong4（ctrl 在首）"
        );
    }

    #[test]
    fn module_path_extraction() {
        assert_eq!(module_path(&[".".into(), "files/".into()], "files"), "");
        assert_eq!(
            module_path(&[".".into(), "files/sub/".into()], "files"),
            "sub"
        );
        assert_eq!(module_path(&[".".into()], "files"), "");
    }

    #[test]
    fn args_parse_recursive_and_e_flags() {
        let segs: Vec<String> = [
            "--server",
            "--sender",
            "-de.LsfxCIvu",
            "--list-only",
            ".",
            "files/",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let a = parse_args(&segs);
        assert!(a.is_sender && a.list_only && !a.recursive);
        assert_eq!(a.client_info, ".LsfxCIvu");
        assert_eq!(compute_compat(&a.client_info), 0x1FE, "真机 compat 值");

        let segs2: Vec<String> = ["--server", "--sender", "-rde.iLsfxCIvu", ".", "files/"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let b = parse_args(&segs2);
        assert!(b.recursive, "-r 在簇内");
        assert_eq!(b.client_info, ".iLsfxCIvu");
    }

    #[test]
    fn parses_version_lines() {
        assert_eq!(parse_version("@RSYNCD: 30.0\n"), Some(30));
        assert_eq!(parse_version("@RSYNCD: 31.0 sha512\n"), Some(31));
        assert_eq!(parse_version("garbage"), None);
    }

    /// 模块列表与未知模块（Phase0 形 ✗ 保持既有黄金）。
    #[tokio::test]
    async fn module_list_and_unknown_module() {
        let (client, server) = tokio::io::duplex(4096);
        tokio::spawn(async move {
            let backend = FakeBackend::new(Vec::new(), Vec::new());
            handle_conn(server, "files", AuthConfig::default(), &backend)
                .await
                .unwrap()
        });
        let mut c = BufReader::new(client);
        let mut line = String::new();
        c.read_line(&mut line).await.unwrap();
        assert_eq!(line, PROTOCOL_LINE);
        c.get_mut().write_all(b"@RSYNCD: 31.0\n").await.unwrap();
        c.get_mut().write_all(b"\n").await.unwrap();
        line.clear();
        c.read_line(&mut line).await.unwrap();
        assert_eq!(line, format!("{:<15}\t\n", "files"));
        line.clear();
        c.read_line(&mut line).await.unwrap();
        assert_eq!(line, "@RSYNCD: EXIT\n");
    }

    /// 整链 duplex：真机写序列 → 断言服务端应答的 compat/协商/seed/flist/收尾。
    #[tokio::test]
    async fn full_list_only_dialogue() {
        let mtime = 0x6AB3A68C_i64;
        let entries = vec![
            FlatEntry {
                name: ".".into(),
                is_dir: true,
                size: 4096,
                mtime,
                mode: 0o40775,
                fs_path: String::new(),
                file_sum: None,
            },
            FlatEntry {
                name: "a.txt".into(),
                is_dir: false,
                size: 6,
                mtime,
                mode: 0o100664,
                fs_path: String::new(),
                file_sum: None,
            },
        ];
        let (client, server) = tokio::io::duplex(64 * 1024);
        let server_entries = entries.clone();
        tokio::spawn(async move {
            let backend = FakeBackend::new(server_entries, Vec::new());
            handle_conn(server, "files", AuthConfig::default(), &backend)
                .await
                .unwrap()
        });
        let mut c = BufReader::new(client);
        // banner + 选模块
        c.get_mut()
            .write_all(b"@RSYNCD: 31.0 sha512 sha256 sha1 md5 md4\n")
            .await
            .unwrap();
        let mut line = String::new();
        c.read_line(&mut line).await.unwrap();
        assert_eq!(line, PROTOCOL_LINE);
        c.get_mut().write_all(b"files\n").await.unwrap();
        line.clear();
        c.read_line(&mut line).await.unwrap();
        assert_eq!(line, "@RSYNCD: OK\n");
        // args（真机字面）
        for a in [
            "--server\0",
            "--sender\0",
            "-de.LsfxCIvu\0",
            "--list-only\0",
            ".\0",
            "files/\0",
            "\0",
        ] {
            c.get_mut().write_all(a.as_bytes()).await.unwrap();
        }
        // compat_flags = 0x1FE → 81 FE
        let mut ack = [0u8; 2];
        c.read_exact(&mut ack).await.unwrap();
        assert_eq!(ack, [0x81, 0xFE], "compat_flags varint");
        // 服务端 checksum vstring = # + 35B
        let mut head = [0u8; 1];
        c.read_exact(&mut head).await.unwrap();
        assert_eq!(head[0] as usize, CHECKSUM_LIST.len());
        let mut list = vec![0u8; CHECKSUM_LIST.len()];
        c.read_exact(&mut list).await.unwrap();
        assert_eq!(&list, CHECKSUM_LIST.as_bytes());
        // 客户端 checksum vstring（回写）
        c.get_mut().write_all(b"\x1e").await.unwrap();
        c.get_mut()
            .write_all(b"xxh128 xxh3 xxh64 md5 md4 sha1")
            .await
            .unwrap();
        // checksum_seed 4B
        let mut seed = [0u8; 4];
        c.read_exact(&mut seed).await.unwrap();
        // filter list（mux int 0）
        c.get_mut()
            .write_all(&mux_frame(&0i32.to_le_bytes()))
            .await
            .unwrap();
        // flist 帧
        let expected = encode_flist(&entries, true);
        let mut hdr = [0u8; 4];
        c.read_exact(&mut hdr).await.unwrap();
        let flen = (hdr[0] as usize) | ((hdr[1] as usize) << 8) | ((hdr[2] as usize) << 16);
        assert_eq!(hdr[3], MPLEX_BASE + MSG_DATA);
        assert_eq!(flen, expected.len());
        let mut body = vec![0u8; flen];
        c.read_exact(&mut body).await.unwrap();
        assert_eq!(body, expected, "flist payload 逐字节");
        // 收尾：发 3 个 NDX_DONE（真机写序列）
        c.get_mut().write_all(&mux_frame(&[0u8])).await.unwrap();
        c.get_mut().write_all(&mux_frame(&[0u8])).await.unwrap();
        c.get_mut().write_all(&mux_frame(&[0u8])).await.unwrap();
        // 收 2×NDX_DONE + 终结 NDX_DONE + 统计
        for _ in 0..3 {
            let mut h = [0u8; 4];
            c.read_exact(&mut h).await.unwrap();
            let l = (h[0] as usize) | ((h[1] as usize) << 8) | ((h[2] as usize) << 16);
            let mut p = vec![0u8; l];
            c.read_exact(&mut p).await.unwrap();
            assert_eq!(l, 1, "收尾 NDX_DONE");
            assert_eq!(p[0], 0);
        }
        let mut h = [0u8; 4];
        c.read_exact(&mut h).await.unwrap();
        let l = (h[0] as usize) | ((h[1] as usize) << 8) | ((h[2] as usize) << 16);
        assert_eq!(l, 15, "5×varlong30(3) 统计");
        let mut stats = vec![0u8; 15];
        c.read_exact(&mut stats).await.unwrap();
        // 最后问候
        c.get_mut().write_all(&mux_frame(&[0u8])).await.unwrap();
    }

    /// 排序 = rsync `f_name_cmp` 真机序（文件先于目录、各按名升序、遇目录深度优先下钻）。
    #[test]
    fn sort_flist_matches_rsync_order() {
        let mut e = vec![
            FlatEntry::dir("mid", 1),
            FlatEntry::file("mid/inner.txt", 1, 1),
            FlatEntry::file("zeta.txt", 1, 1),
            FlatEntry::dir("alpha", 1),
            FlatEntry::file("alpha/a.txt", 1, 1),
            FlatEntry::dir("alpha/inner", 1),
            FlatEntry::file("beta.txt", 1, 1),
            FlatEntry::dir(".", 1),
        ];
        sort_flist(&mut e);
        let names: Vec<&str> = e.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                ".",
                "beta.txt",
                "zeta.txt",
                "alpha",
                "alpha/a.txt",
                "alpha/inner",
                "mid",
                "mid/inner.txt"
            ],
            "深度优先：文件先于目录"
        );
    }

    /// 整文件下载整链（真机请求帧 0xA000 + 全零 sum_head → 断言数据帧逐字节）。
    #[tokio::test]
    async fn whole_file_download_dialogue() {
        let content = b"tiny-content-12345\n".to_vec();
        let entries = vec![FlatEntry::file("tiny.txt", 19, 0x6AB3A68C).with_fs_path("tiny.txt")];
        let (client, server) = tokio::io::duplex(64 * 1024);
        let server_entries = entries.clone();
        let server_content = content.clone();
        tokio::spawn(async move {
            let backend =
                FakeBackend::new(server_entries, vec![("tiny.txt".into(), server_content)]);
            handle_conn(server, "files", AuthConfig::default(), &backend)
                .await
                .unwrap()
        });
        let mut c = BufReader::new(client);
        c.get_mut()
            .write_all(b"@RSYNCD: 31.0 sha512 sha256 sha1 md5 md4\n")
            .await
            .unwrap();
        let mut line = String::new();
        c.read_line(&mut line).await.unwrap();
        c.get_mut().write_all(b"files\n").await.unwrap();
        line.clear();
        c.read_line(&mut line).await.unwrap();
        assert_eq!(line, "@RSYNCD: OK\n");
        for a in [
            "--server\0",
            "--sender\0",
            "-e.LsfxCIvu\0",
            ".\0",
            "files/tiny.txt\0",
            "\0",
        ] {
            c.get_mut().write_all(a.as_bytes()).await.unwrap();
        }
        let mut ack = [0u8; 2];
        c.read_exact(&mut ack).await.unwrap();
        assert_eq!(ack, [0x81, 0xFE]);
        // checksum 清单 vstring（md5 = 3 字节）
        let mut head = [0u8; 1];
        c.read_exact(&mut head).await.unwrap();
        assert_eq!(head[0] as usize, CHECKSUM_LIST.len());
        let mut list = vec![0u8; CHECKSUM_LIST.len()];
        c.read_exact(&mut list).await.unwrap();
        assert_eq!(&list, CHECKSUM_LIST.as_bytes());
        c.get_mut().write_all(b"\x1e").await.unwrap();
        c.get_mut()
            .write_all(b"xxh128 xxh3 xxh64 md5 md4 sha1")
            .await
            .unwrap();
        let mut seed = [0u8; 4];
        c.read_exact(&mut seed).await.unwrap();
        // filter list
        c.get_mut()
            .write_all(&mux_frame(&0i32.to_le_bytes()))
            .await
            .unwrap();
        // flist（单文件 = 无 `.`）
        let expected_flist = encode_flist(&entries, true);
        let mut hdr = [0u8; 4];
        c.read_exact(&mut hdr).await.unwrap();
        let flen = (hdr[0] as usize) | ((hdr[1] as usize) << 8) | ((hdr[2] as usize) << 16);
        let mut body = vec![0u8; flen];
        c.read_exact(&mut body).await.unwrap();
        assert_eq!(body, expected_flist);
        // 请求帧：ndx 0 + iflags 0xA000 + 全零 sum_head（真机字面）
        let mut req = vec![0x01, 0x00, 0xA0];
        req.extend_from_slice(&[0u8; 16]);
        c.get_mut().write_all(&mux_frame(&req)).await.unwrap();
        // 数据帧
        let mut h = [0u8; 4];
        c.read_exact(&mut h).await.unwrap();
        let l = (h[0] as usize) | ((h[1] as usize) << 8) | ((h[2] as usize) << 16);
        assert_eq!(h[3], MPLEX_BASE + MSG_DATA);
        let mut data = vec![0u8; l];
        c.read_exact(&mut data).await.unwrap();
        // ndx 回显 + iflags + sum_head(16 零) + literal 长度 + 内容 + 终结 + md5
        let mut want = vec![0x01, 0x00, 0xA0];
        want.extend_from_slice(&[0u8; 16]);
        want.extend_from_slice(&(content.len() as i32).to_le_bytes());
        want.extend_from_slice(&content);
        want.extend_from_slice(&0i32.to_le_bytes());
        want.extend_from_slice(&md5_digest(&content));
        assert_eq!(data, want, "数据帧逐字节（真机转录同形）");
        assert_eq!(
            &data[want.len() - 16..],
            &md5_digest(b"tiny-content-12345\n"),
            "校验和 = MD5(内容) 无 seed"
        );
        // 收尾
        c.get_mut().write_all(&mux_frame(&[0u8])).await.unwrap();
        c.get_mut().write_all(&mux_frame(&[0u8])).await.unwrap();
        c.get_mut().write_all(&mux_frame(&[0u8])).await.unwrap();
        for _ in 0..3 {
            let mut hh = [0u8; 4];
            c.read_exact(&mut hh).await.unwrap();
            let ll = (hh[0] as usize) | ((hh[1] as usize) << 8) | ((hh[2] as usize) << 16);
            let mut p = vec![0u8; ll];
            c.read_exact(&mut p).await.unwrap();
        }
        let mut hh = [0u8; 4];
        c.read_exact(&mut hh).await.unwrap();
        let ll = (hh[0] as usize) | ((hh[1] as usize) << 8) | ((hh[2] as usize) << 16);
        let mut stats = vec![0u8; ll];
        c.read_exact(&mut stats).await.unwrap();
        c.get_mut().write_all(&mux_frame(&[0u8])).await.unwrap();
    }

    /// md5 与 RFC 1321 向量（rsync 整文件校验和同算法）。
    #[test]
    fn md5_known_vector() {
        assert_eq!(
            md5_digest(b"abc"),
            [
                0x90, 0x01, 0x50, 0x98, 0x3c, 0xd2, 0x4f, 0xb0, 0xd6, 0x96, 0x3f, 0x7d, 0x28, 0xe1,
                0x7f, 0x72
            ]
        );
    }

    /// `-a` 关键面：flist 的 uid/gid 字段 + 其后的 uid/gid 名列表（r18 真机 bug 回归）。
    #[tokio::test]
    async fn flist_owner_fields_and_id_list_parse() {
        let mut body = Vec::new();
        write_varint(1 << 2, &mut body); // xflags = EXTENDED_FLAGS（非 0 = 非终结；无 SAME_*）
        body.push(5);
        body.extend_from_slice(b"f.txt");
        write_varlong(3, 5, &mut body); // size
        write_varlong(4, 1_700_000_000, &mut body); // mtime
        body.extend_from_slice(&0o100644u32.to_le_bytes());
        write_varint(1000, &mut body); // uid
        write_varint(1000, &mut body); // gid
        write_varint(0, &mut body); // 终结 xflags
        write_varint(0, &mut body); // io_error
        // 两条 id 列表（uid/gid 对称）+ 终结（xmit_id0_names = varint(0)+byte(0)）
        for _ in 0..2 {
            write_varint(1000, &mut body);
            body.push(4);
            body.extend_from_slice(b"user");
            write_varint(0, &mut body);
            body.push(0);
        }
        let (mut c, srv) = tokio::io::duplex(64 * 1024);
        c.write_all(&mux_frame(&body)).await.unwrap();
        c.shutdown().await.unwrap();
        let mut rw = BufReader::new(srv);
        let mut pending = Vec::new();
        let got = recv_file_list(
            &mut rw,
            &mut pending,
            FlistOpts {
                varint_flags: true,
                preserve_uid: true,
                preserve_gid: true,
                ..Default::default()
            },
        )
        .await
        .expect("uid/gid 字段解析不得错位");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "f.txt");
        assert_eq!(got[0].size, 5);
        assert_eq!(got[0].mtime, 1_700_000_000);
        assert_eq!(got[0].mode, 0o100644);
        skip_id_list(&mut rw, &mut pending, true).await.unwrap();
        skip_id_list(&mut rw, &mut pending, true).await.unwrap();
    }

    /// filter 通配匹配（`*` 不跨 `/`、`**` 跨、`?`、字符类）。
    #[test]
    fn wildmatch_subset_matches_rsync() {
        assert!(wildmatch("*.tmp", "a.tmp"));
        assert!(!wildmatch("*.tmp", "a.txt"));
        assert!(wildmatch("sub/*.tmp", "sub/a.tmp"));
        assert!(!wildmatch("sub/*.tmp", "sub/deep/a.tmp"), "* 不跨 /");
        assert!(wildmatch("sub/**/*.tmp", "sub/deep/a.tmp"), "** 跨 /");
        assert!(wildmatch("a?c", "abc"));
        assert!(wildmatch("[ab]c", "bc"));
        assert!(wildmatch("[!a]c", "bc"));
        assert!(!wildmatch("[!b]c", "bc"));
        assert!(wildmatch("*", "anything"));
        assert!(wildmatch("exact", "exact"));
        assert!(!wildmatch("exact", "exact2"));
    }

    /// 规则解析（`-`/`+`、目录尾 `/`、锚定首 `/`）+ 首条命中语义 + 保护判定。
    #[test]
    fn filter_rules_parse_and_protect() {
        let r = parse_rule("- *.tmp").unwrap();
        assert!(!r.include && !r.dir_only && !r.anchored && r.pattern == "*.tmp");
        let r = parse_rule("+ keep.tmp").unwrap();
        assert!(r.include);
        let r = parse_rule("- cache/").unwrap();
        assert!(r.dir_only, "尾 / = 仅目录");
        let r = parse_rule("- /top.txt").unwrap();
        assert!(r.anchored, "首 / = 锚定");
        let r = parse_rule("- sub/x.txt").unwrap();
        assert!(r.anchored, "含 / = 锚定");
        assert!(parse_rule(": merge").is_none(), "不支持类型跳过");
        // flags 前缀剥离（`P` 类保护规则序列化为 `-r pat` ✗ 空格分隔）
        let r = parse_rule("-r *.probe").expect("flags 形可解析");
        assert!(!r.include && r.pattern == "*.probe", "flags 不进 pattern");
        assert_eq!(
            parse_rule("+s my file.txt").map(|r| (r.include, r.pattern)),
            Some((true, "my file.txt".to_string())),
            "pattern 内空格保留"
        );

        let rules: Vec<FilterRule> = ["+ keep.tmp", "- *.tmp", "- cache/", "- /top.txt"]
            .iter()
            .filter_map(|l| parse_rule(l))
            .collect();
        assert!(
            !is_excluded(&rules, "keep.tmp", false),
            "先 include 解除保护"
        );
        assert!(is_excluded(&rules, "sub/a.tmp", false), "basename 命中");
        assert!(is_excluded(&rules, "cache", true), "目录保护");
        assert!(!is_excluded(&rules, "cache", false), "非目录不命中目录规则");
        assert!(is_excluded(&rules, "top.txt", false), "锚定命中根");
        assert!(
            !is_excluded(&rules, "sub/top.txt", false),
            "锚定不匹配子路径"
        );
        assert!(!is_excluded(&rules, "other.txt", false), "无命中 = 不保护");
    }

    /// 收端 delta 闭环：basis → 块校验和 → 发送端 token → `apply_tokens` 重建 == 新内容。
    #[test]
    fn receive_delta_roundtrip() {
        let seed = 0xDEAD_BEEFu32;
        let basis: Vec<u8> = (0..20000u32).map(|i| (i % 251) as u8).collect();
        let bl = block_size(basis.len());
        let s2len = 16usize;
        let (count, remainder, _blocks_bytes) = build_block_sums(&basis, bl, seed, s2len);

        // 改动中部 137 字节（保持块对齐 = 只有跨界块失配）
        let mut modified = basis.clone();
        for i in 8000..8137 {
            modified[i] = modified[i].wrapping_add(7);
        }
        let blocks: Vec<BlockSum> = {
            let b = bl as usize;
            (0..count as usize)
                .map(|i| {
                    let start = i * b;
                    let len = if i == count as usize - 1 && remainder != 0 {
                        remainder as usize
                    } else {
                        b
                    };
                    let chunk = &basis[start..start + len];
                    let (s1, s2) = checksum1_signed(chunk);
                    BlockSum {
                        sum1: (s1 & 0xffff) | (s2 << 16),
                        sum2: md5_seeded(seed, chunk)[..s2len].to_vec(),
                        len: len as u32,
                    }
                })
                .collect()
        };
        // 发送端视角：由 basis 块校验和编码 token 流
        let tokens = build_delta_tokens(&modified, &blocks, bl, seed, s2len);
        // 接收端视角：由 token 流 + basis 重建
        let rebuilt = apply_tokens(&tokens, &basis, bl, count, remainder).expect("rebuild ok");
        assert_eq!(rebuilt, modified, "收端 delta 重建逐字节同");
        // basis 完全为空时纯 literal 亦通
        let empty = apply_tokens(
            &build_delta_tokens(&modified, &[], 0, seed, s2len),
            &[],
            0,
            0,
            0,
        )
        .unwrap();
        assert_eq!(empty, modified, "无 basis 全 literal");
    }

    /// 认证响应值与真机转录逐字同（sha512 ✗ 客户端 `RSYNC_PASSWORD` 实测帧）。
    #[test]
    fn auth_response_matches_reference_capture() {
        let challenge = "FljsugF4lOnqto/b1XkF+96pGFJKDjtu";
        let resp = "ep/Gj2YdDH/fXg6glclLxU0ZqTxuYuPfDPAcfewF/2O2KIKYU3W5uCYuvEls8rdhnK2T7NH6akWymJ4lGzWFBw";
        assert_eq!(
            auth_response("s3cret", challenge, "sha512"),
            resp,
            "sha512 响应"
        );
        let mut secrets = std::collections::HashMap::new();
        secrets.insert("alice".to_string(), "s3cret".to_string());
        let auth = AuthConfig {
            users: vec!["alice".to_string(), "bob:ro".to_string()],
            secrets,
            writable: true,
        };
        assert_eq!(
            auth.verify("alice", resp, challenge, "sha512"),
            Some(true),
            "alice 可写"
        );
        assert_eq!(auth.verify("alice", "bogus", challenge, "sha512"), None);
        assert_eq!(auth.verify("carol", resp, challenge, "sha512"), None);
    }

    /// wire 读端与写端互逆（varint / varlong ✗ push 收 flist 前置）。
    #[tokio::test]
    async fn wire_readers_invert_writers() {
        for v in [0i32, 1, 0x19, 0x9A, 0x1FE, 0x7FFF, 1 << 20] {
            let mut enc = Vec::new();
            write_varint(v, &mut enc);
            let (mut c, srv) = tokio::io::duplex(64);
            c.write_all(&mux_frame(&enc)).await.unwrap();
            c.shutdown().await.unwrap();
            let mut rw = BufReader::new(srv);
            let mut pending = Vec::new();
            assert_eq!(data_varint(&mut rw, &mut pending).await.unwrap(), v);
        }
        for v in [0i64, 6, 4096, 0x6AB3A68C, 0x1234567] {
            let mut enc = Vec::new();
            write_varlong(3, v, &mut enc);
            let (mut c, srv) = tokio::io::duplex(64);
            c.write_all(&mux_frame(&enc)).await.unwrap();
            c.shutdown().await.unwrap();
            let mut rw = BufReader::new(srv);
            let mut pending = Vec::new();
            assert_eq!(data_varlong(&mut rw, &mut pending, 3).await.unwrap(), v);
        }
    }

    /// flist 编解码互逆（encode_flist → recv_file_list ✗ push 收端解码器）。
    #[tokio::test]
    async fn flist_encode_recv_roundtrip() {
        let entries = vec![
            FlatEntry::dir(".", 1),
            FlatEntry::file("a.txt", 5, 2),
            FlatEntry::dir("sub", 2),
            FlatEntry::file("sub/b.txt", 7, 2),
            FlatEntry::file("中文名.txt", 9, 3),
        ];
        let encoded = encode_flist(&entries, true);
        let (mut c, srv) = tokio::io::duplex(64 * 1024);
        c.write_all(&mux_frame(&encoded)).await.unwrap();
        c.shutdown().await.unwrap();
        let mut rw = BufReader::new(srv);
        let mut pending = Vec::new();
        let got = recv_file_list(
            &mut rw,
            &mut pending,
            FlistOpts {
                varint_flags: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        // `-c`：普通文件末附 16B 整文件校验和（目录不附 ✗ 对端同语义）
        let file_sum = Some(md5_digest(b"hello-rsync").to_vec());
        let mut with_sum = entries.clone();
        for e in with_sum.iter_mut() {
            if !e.is_dir {
                e.file_sum = file_sum.clone();
            }
        }
        let enc2 = encode_flist(&with_sum, true);
        let (mut c2, srv2) = tokio::io::duplex(64 * 1024);
        c2.write_all(&mux_frame(&enc2)).await.unwrap();
        c2.shutdown().await.unwrap();
        let mut rw2 = BufReader::new(srv2);
        let mut p2 = Vec::new();
        let got2 = recv_file_list(
            &mut rw2,
            &mut p2,
            FlistOpts {
                varint_flags: true,
                always_checksum: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(got2.len(), with_sum.len(), "-c 条目数");
        for (a, b) in got2.iter().zip(with_sum.iter()) {
            assert_eq!(a.file_sum, b.file_sum, "-c 校验和");
        }
        assert!(
            got2.iter()
                .filter(|e| e.is_dir)
                .all(|e| e.file_sum.is_none())
        );
        assert_eq!(got.len(), entries.len(), "条目数");
        for (a, b) in got.iter().zip(entries.iter()) {
            assert_eq!(a.name, b.name, "名字");
            assert_eq!(a.is_dir, b.is_dir, "目录位");
            assert_eq!(a.size, b.size, "大小");
            assert_eq!(a.mode, b.mode, "mode");
        }
    }

    /// 弱/强校验和与真机转录逐位同（官方 daemon md5 + 200000B 模式文件第 0 块）。
    #[test]
    fn rolling_and_seeded_md5_match_reference_capture() {
        let pat: Vec<u8> = (0..700).map(|i| ((i * 7 + 3) % 256) as u8).collect();
        let (s1, s2) = checksum1_signed(&pat);
        assert_eq!((s1 & 0xffff) | (s2 << 16), 0x760c_feda, "真机块 0 弱校验和");
        let seed = u32::from_le_bytes([0x76, 0x9d, 0x58, 0x67]);
        assert_eq!(
            &md5_seeded(seed, &pat)[..2],
            &[0x25, 0x91],
            "真机块 0 强校验和前缀（MD5(seed‖block)）"
        );
    }

    fn blocks_of(basis: &[u8], blength: u32, seed: u32, s2len: usize) -> Vec<BlockSum> {
        let b = blength as usize;
        let count = basis.len().div_ceil(b);
        (0..count)
            .map(|i| {
                let start = i * b;
                let end = (start + b).min(basis.len());
                let chunk = &basis[start..end];
                let (s1, s2) = checksum1_signed(chunk);
                BlockSum {
                    sum1: (s1 & 0xffff) | (s2 << 16),
                    sum2: md5_seeded(seed, chunk)[..s2len].to_vec(),
                    len: (end - start) as u32,
                }
            })
            .collect()
    }

    /// 同一 basis → 全匹配（token 流无 literal，仅匹配 token + 终结 0）。
    #[test]
    fn delta_full_match_for_identical_content() {
        let seed = 0x6718_9d76u32;
        let data: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        let blength = 700u32;
        let blocks = blocks_of(&data, blength, seed, 16);
        let tokens = build_delta_tokens(&data, &blocks, blength, seed, 16);
        // 7 个满块 + 尾 100B 短块 = 8 个匹配 token，无 literal
        let mut expect = Vec::new();
        for i in 0..8i32 {
            expect.extend_from_slice(&(-(i + 1)).to_le_bytes());
        }
        expect.extend_from_slice(&0i32.to_le_bytes());
        assert_eq!(tokens, expect, "全匹配 = 8 token + 终结");
    }

    /// basis 中部改 300B → 前后块仍匹配、改动区作 literal。
    #[test]
    fn delta_partial_match_with_literal_run() {
        let seed = 0x1122_3344u32;
        let basis: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        let mut data = basis.clone();
        for i in 1400..1700 {
            data[i] = data[i].wrapping_add(1);
        }
        let blength = 700u32;
        let blocks = blocks_of(&basis, blength, seed, 16);
        let tokens = build_delta_tokens(&data, &blocks, blength, seed, 16);
        // 至少有一个匹配 token（-1/-2/... 或后续）与 literal 长度 >0
        let mut i = 0usize;
        let mut literals = 0usize;
        let mut matches = 0usize;
        while i + 4 <= tokens.len() {
            let v = i32::from_le_bytes([tokens[i], tokens[i + 1], tokens[i + 2], tokens[i + 3]]);
            i += 4;
            if v > 0 {
                literals += v as usize;
                i += v as usize;
            } else if v == 0 {
                break;
            } else {
                matches += 1;
            }
        }
        assert!(matches >= 3, "改动区前后应有多个匹配：matches={matches}");
        assert!(literals >= 300, "改动区 + 对齐余量应作 literal：{literals}");
        // 完整解析到终结
        assert_eq!(i, tokens.len(), "token 流自洽到终结");
    }
}
