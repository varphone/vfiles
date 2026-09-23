//! rsync 协议 daemon（round 3/256 ✗ RSYNC_PLAN 定案：自研 rsync-30 + daemon 形）。
//!
//! Phase0 = 版本交换 + 模块列表/选择两段（`@RSYNCD` 行式明文协议）——**真 rsync CLI
//! 即权威参照**（字节错则它秒教 ✗ 与 s3s 真客户端教学同式）✗ 协议锁 30.0（客户端
//! 3.2.7 协议 31 → 我方回 30.0 协商降级 ✓）。
//!
//! 分层：本 crate = 纯协议件（`handle_conn` 吃 AsyncRead/Write = **duplex 黄金单测
//! 可打** ✗ 字节形断言）/ 装配 = bin 侧（bind + accept loop + r205 降级 + shutdown
//! 接，与 S3 spawn 同式）。

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

/// 协议版本行（锁定 30.0 ✗ 31 客户端协商降级到此）。
/// banner = 版本 + 空格分隔校验和算法串（官方 18873 黄金逐字节 ✗ 锁 30.0 + 客户端 31
/// 降级;算法串 = 官方同款 = 客户端按可用协商）。
pub const PROTOCOL_LINE: &str = "@RSYNCD: 30.0 sha512 sha256 sha1 md5 md4\n";

/// 处理单连接：版本交换 → 模块列表（客户端空行请求）或模块选择（名字行）。
///
/// 行为按真实 rsync daemon 形：服务端先发自身版本行；客户端回自身版本（取 min 即
/// 双方均回一次 = 协议锁定 ✗ 低于 30 拒绝）；空行 = 请求模块列表（名字行 + NUL 终
/// 结）；名字 = 选模块（命中 → `@RSYNCD: OK\n` ✗ 未命中 → ERROR 行）。
pub async fn handle_conn<S>(stream: S, module: &str) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // 单实例 BufReader 包 stream（每行新建会吞行后字节 ✗ 20 分钟死锁真凶 ✗ 写走
    // get_mut 不破坏缓冲 = 教科书式解）。
    let mut rw = BufReader::new(stream);

    // ① 服务端先发自身版本行
    rw.get_mut().write_all(PROTOCOL_LINE.as_bytes()).await?;
    rw.get_mut().flush().await?;

    // ② 客户端版本行（`@RSYNCD: 31.0` 形）
    let mut client_line = String::new();
    let n = rw.read_line(&mut client_line).await?;
    if n == 0 {
        return Ok(()); // 客户端提前断
    }
    match parse_version(&client_line) {
        Some(v) if v >= 30 => {
            // 协议锁 30（我方上限 = 30.0 ✗ 客户端 31 也降级到此 = 双方 min 心智）
        }
        _ => {
            rw.get_mut()
                .write_all(b"@RSYNCD: ERROR unsupported protocol version\n")
                .await?;
            rw.get_mut().flush().await?;
            return Ok(());
        }
    }

    // ③ 请求行：空行 = 模块列表 / 名字 = 选模块
    let mut request = String::new();
    let n = rw.read_line(&mut request).await?;
    if n == 0 {
        return Ok(());
    }
    let text = request.trim_end_matches(['\n', '\r']).to_string();

    if text.is_empty() {
        // 模块列表黄金形（官方 18873 逐字节）：`{name:<15}` + ``\t` + 描述（空）+ ``\n`
        // → `@RSYNCD: EXIT\n` 收尾 + 关连接 ✗ **NUL 是多路复用帧的 channel 字节、
        // 不是列表终结**（r4 黄金对照破的百年疑案 ✗ r3 把它当 terminator = RC5 真因）
        let line = format!("{:<15}\t\n", module);
        rw.get_mut().write_all(line.as_bytes()).await?;
        rw.get_mut().write_all(b"@RSYNCD: EXIT\n").await?;
        rw.get_mut().flush().await?;
        return Ok(());
    }

    if text == module {
        rw.get_mut().write_all(b"@RSYNCD: OK\n").await?;
        rw.get_mut().flush().await?;
        // Phase1（r4）= 命令段 + file_list ✗ r3 到 OK 即完成模块选择面
        Ok(())
    } else {
        // 官方形（r4 对照 ✗ P04 单行即关 = @ERROR: Unknown module '<name>' + 断言同步）
        let line = format!("@ERROR: Unknown module '{}'\n", text);
        rw.get_mut().write_all(line.as_bytes()).await?;
        rw.get_mut().flush().await?;
        Ok(())
    }
}

/// 解析 `@RSYNCD: 30.0` → 主次版本（非版本行 = None）。
fn parse_version(line: &str) -> Option<u32> {
    let rest = line.trim().strip_prefix("@RSYNCD:")?.trim();
    let major = rest.split('.').next()?;
    major.parse().ok()
}

async fn write_all_line<W: AsyncWrite + Unpin>(w: &mut W, line: &str) -> std::io::Result<()> {
    w.write_all(line.as_bytes()).await?;
    w.write_all(b"\n").await?;
    w.flush().await?;
    Ok(())
}

/// 测试侧单手读行（借毕即释 ✗ 防 BufReader 活借跨写 E0499）。


#[cfg(test)]
mod handshake_tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    /// 黄金式（官方 18873 对照）：banner → client 31 → 空行 → `{files:<15}\t\n` + `@RSYNCD: EXIT\n`。
    #[tokio::test]
    async fn lists_module_after_version_handshake() {
        let (client, server) = tokio::io::duplex(4096);
        tokio::spawn(async move { handle_conn(server, "files").await.unwrap() });

        // 单实例 BufReader 包 duplex（写走 get_mut ✗ NUL 走读侧 = 缓冲字节不丢 ✗ 死锁根治）
        let mut c = BufReader::new(client);

        let mut line = String::new();
        c.read_line(&mut line).await.unwrap();
        assert_eq!(line, PROTOCOL_LINE, "服务端先发 30.0");

        c.get_mut().write_all(b"@RSYNCD: 31.0\n").await.unwrap();
        c.get_mut().write_all(b"\n").await.unwrap(); // 请求模块列表

        line.clear();
        c.read_line(&mut line).await.unwrap();
        assert_eq!(line, format!("{:<15}\t\n", "files"), "模块行黄金形（15列+tab）");
        // 收尾黄金 = @RSYNCD: EXIT 行 + EOF（NUL 是 mux 帧字节 ✗ r4 对照破的错位）
        line.clear();
        c.read_line(&mut line).await.unwrap();
        assert_eq!(line, "@RSYNCD: EXIT\n", "EXIT 收尾");
    }

    /// 黄金式：选中模块 = OK 行 / 未命中 = ERROR 行。
    #[tokio::test]
    async fn selects_module_ok_and_unknown_error() {
        let (client, server) = tokio::io::duplex(4096);
        tokio::spawn(async move { handle_conn(server, "files").await.unwrap() });
        let mut c = BufReader::new(client);

        let mut line = String::new();
        c.read_line(&mut line).await.unwrap(); // 版本行
        c.get_mut().write_all(b"@RSYNCD: 30.0\n").await.unwrap();
        c.get_mut().write_all(b"files\n").await.unwrap();
        line.clear();
        c.read_line(&mut line).await.unwrap();
        assert_eq!(line, "@RSYNCD: OK\n");

        let (c2, s2) = tokio::io::duplex(4096);
        tokio::spawn(async move { handle_conn(s2, "files").await.unwrap() });
        let mut r2 = BufReader::new(c2);
        let mut l2 = String::new();
        r2.read_line(&mut l2).await.unwrap();
        r2.get_mut().write_all(b"@RSYNCD: 30.0\n").await.unwrap();
        r2.get_mut().write_all(b"nope\n").await.unwrap();
        l2.clear();
        r2.read_line(&mut l2).await.unwrap();
        assert_eq!(l2, "@ERROR: Unknown module 'nope'\n", "官方 P04 单行形");
    }

    /// 版本解析：30/31 过、垃圾拒。
    #[test]
    fn parses_version_lines() {
        assert_eq!(parse_version("@RSYNCD: 30.0\n"), Some(30));
        assert_eq!(parse_version("@RSYNCD: 31.0\n"), Some(31));
        assert_eq!(parse_version("garbage"), None);
    }
}
