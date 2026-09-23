# rsync 协议 daemon（round 2 选型 → **round 3 Phase0 探底** ✗ 状态见下）

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
