# rsync 协议 daemon（round 2 选型 → **round 3 Phase0 探底** ✗ 状态见下）

## 状态（r3 末 · 真 rsync 3.2.7 实证）

- **已通**：版本协商（服务端 `@RSYNCD: 30.0` banner + 客户端 31 降级）✗ **`rsync
  --list-only rsync://host:PORT/` 真打印模块名 `files`** ✓ 未知模块回
  `@RSYNCD: ERROR Unknown module` ✓ python 裸 socket 字节断言（banner/`files`）✓ 三黄金
  单测（死锁根治后 0.00s 绿）+ 装配（`VFILES_RSYNC_ENABLED` 默认关 / PORT / MODULE 三
  env + accept loop select 停机 + r205 降级 + 关时日志明示启用法）。
- **差一层（r4 首步）**：客户端列表后报 `didn't get server startup line (code5 @
  main.c:1885)` = **收尾序列期待差一点** → r4 开工 = **本机起官方 `rsync --daemon` 抓
  黄金字节序列对照**（官方参照本地就有 = 最强对齐法）→ 然后 file_list 编码 → delta
  下载（r5）。
- Phase1（命令段 + file_list）/ Phase2（delta）未开 ✗ 分层推进；探针教学习惯 =
  **真 CLI 输出全真文呈现、按真形判**（零预设 ✗ 三式已证伪两层预设）。

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
