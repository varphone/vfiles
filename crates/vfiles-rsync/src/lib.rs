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

use std::future::Future;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use vfiles_domain::{EntryKind, NamespaceId};

/// daemon banner（版本 + 算法串 ✗ 官方 `output_daemon_greeting` 形）。
pub const PROTOCOL_LINE: &str = "@RSYNCD: 30.0 sha512 sha256 sha1 md5 md4\n";
/// 服务端校验和清单（官方 valid_checksums 默认序 = 最强优先；尾 `none` 仅服务端列出）。
const CHECKSUM_LIST: &str = "xxh128 xxh3 xxh64 md5 md4 sha1 none";
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
        }
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

// ─────────────────────────── args 解析 ───────────────────────────

/// 客户端 args 解析结果。
#[derive(Debug, Default, Clone)]
struct ParsedArgs {
    is_sender: bool,
    recursive: bool,
    compress: bool,
    list_only: bool,
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
            ((two[0] as u32) << 8) + two[1] as u32 + *prev_positive as u32
        }
    } else {
        b[0] as u32 + *prev_positive as u32
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

/// 处理一条 rsync daemon 连接（协议 30）。
///
/// `list` 在为 sender 模式解析完 args 后调用一次，返回该请求的条目流（`.` 必须首条）。
pub async fn handle_conn<S, F, Fut>(stream: S, module: &str, list: F) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
    F: FnOnce(ListRequest) -> Fut,
    Fut: Future<Output = Result<Vec<FlatEntry>, String>>,
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
    write_raw(&mut rw, b"@RSYNCD: OK\n").await?;

    // ③ args（NUL 分隔、空段终结）
    let segs = read_arg_segments(&mut rw).await?;
    let args = parse_args(&segs);

    if !args.is_sender {
        // 收端（push）尚未支持：模块只读。给出协议级错误后关闭。
        tracing::warn!("rsync：收到 push 请求，但模块当前只读（未实现收端）");
        return Ok(());
    }

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

    // ⑤ 多路复用输入开启：filter list（官方 recv_filter_list：int len + 规则，0 终结）
    let mut pending: Vec<u8> = Vec::new();
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

    // ⑥ flist：收集 + 编码 + 发送
    let req = ListRequest {
        recursive: args.recursive,
        path: module_path(&args.paths, module),
    };
    let entries = match list(req).await {
        Ok(entries) => entries,
        Err(err) => {
            tracing::warn!(error = %err, "rsync：列举失败，回空清单");
            vec![FlatEntry::dir(".", now_unix())]
        }
    };
    let flist = encode_flist(&entries, negotiated);
    let total_size: u64 = entries.iter().filter(|e| !e.is_dir).map(|e| e.size).sum();
    write_msg(&mut rw, &flist).await?;

    // ⑦ 收尾对答：3×NDX_DONE 入 → 2×NDX_DONE + 终结 NDX_DONE 出 → 统计 → 最后问候
    let mut phase = 0;
    let mut prev_positive = -1i32;
    let mut prev_negative = 1i32;
    loop {
        let ndx = read_ndx(
            &mut rw,
            &mut pending,
            &mut prev_positive,
            &mut prev_negative,
        )
        .await?;
        if ndx == NDX_DONE {
            phase += 1;
            if phase > 2 {
                break;
            }
            write_msg(&mut rw, &[0u8]).await?;
        } else {
            tracing::warn!(ndx = ndx, "rsync：请求具体文件（delta 面未实现），结束会话");
            break;
        }
    }
    write_msg(&mut rw, &[0u8]).await?;

    let mut stats = Vec::new();
    write_varlong(3, 0, &mut stats); // total_read
    write_varlong(3, flist.len() as i64, &mut stats); // total_written（近似）
    write_varlong(3, total_size as i64, &mut stats);
    write_varlong(3, 1, &mut stats); // flist_buildtime
    write_varlong(3, 0, &mut stats); // flist_xfertime
    write_msg(&mut rw, &stats).await?;

    // 最后一个 NDX_DONE 问候（官方 read_final_goodbye 协议 30 分支）
    let _ = read_ndx(
        &mut rw,
        &mut pending,
        &mut prev_positive,
        &mut prev_negative,
    )
    .await?;
    Ok(())
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
    let base_mtime = if req.path.is_empty() {
        now_unix()
    } else {
        match repo.find_by_path(namespace, &base).await {
            Ok(Some(entry)) => entry.created_at.unix_timestamp(),
            Ok(None) => return Err(format!("路径不存在: {}", req.path)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let mut out = vec![FlatEntry::dir(".", base_mtime)];
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
        let mtime = m.entry.created_at.unix_timestamp();
        if is_dir {
            out.push(FlatEntry::dir(name.clone(), mtime));
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
            out.push(FlatEntry::file(name, m.size_bytes.unwrap_or(0), mtime));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(o[0], CHECKSUM_LIST.len() as u8, "35 → 0x23 (#)");
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
            },
            FlatEntry {
                name: "sub".into(),
                is_dir: true,
                size: 4096,
                mtime,
                mode: 0o40775,
            },
            FlatEntry {
                name: "a.txt".into(),
                is_dir: false,
                size: 6,
                mtime,
                mode: 0o100664,
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
            handle_conn(server, "files", |_| async { Ok(Vec::new()) })
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
            },
            FlatEntry {
                name: "a.txt".into(),
                is_dir: false,
                size: 6,
                mtime,
                mode: 0o100664,
            },
        ];
        let (client, server) = tokio::io::duplex(64 * 1024);
        let server_entries = entries.clone();
        tokio::spawn(async move {
            handle_conn(server, "files", move |_req| {
                let e = server_entries.clone();
                async move { Ok(e) }
            })
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
}
