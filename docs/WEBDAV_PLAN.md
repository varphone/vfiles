# WebDAV 支持计划（RFC 4918 子集）

> 状态：**r102 架构定案 + crate 骨架**（本文件 = 实施契约 ✓ r103 = server 全写）。
> 目标：挂载入 Finder / Windows 映射驱动器 / rclone / Cyberduck / davfs2 通用生态。

## 1. 架构定案（r102 侦察实录）

| 面 | 定案 | 依据 |
| --- | --- | --- |
| 形态 | **独立 crate `vfiles-webdav`**（与 vfiles-ftp 协议族并列 ✓ 独立端口/开关 ✓ env 对称 `VFILES_WEBDAV_ENABLED/PORT`） | bin 挂载式 = `spawn_ftp_server` 同构 |
| HTTP 栈 | **axum + tower**（PROPFIND 等非标方法 → `any` 路由 ✓） | vfiles-http 同栈 ✓ |
| auth | **`AuthService::verify_credentials(username, &password)` 复用**（Basic 头 ✓ 与 Web/FTP 完全一致） | vfiles-ftp/src/auth.rs 范本 |
| 存储 | **`BackendDeps`**（`entry_repo`/`snapshot_repo`/`blob_store`/`workspace` ✓ PROPFIND=entry 查询、GET=blob 读） | vfiles-ftp/src/backend.rs:90 |

## 2. 方法集切分

| 轮 | 方法 | 说明 |
| --- | --- | --- |
| r102（已定案） | **OPTIONS / PROPFIND（Depth 0/1）/ GET / HEAD** | 只读四法 ✓ multistatus XML |
| **r103（中段实件 ✓）** | OPTIONS **实装**（Allow + DAV:1 ✓ curl 可证）；auth 解码实 + multistatus XML 全实（4 单测 ✓）；PROPFIND = 501 诚实占位（待域接线） | `cargo test` 4 绿 |
| r104 | PROPFIND 域接线（entry_repo）+ GET（版本链）+ 写法五件 → **可挂载** | 前置：namespace 解析链（backend 侧） |
| r103 | **PUT / DELETE / MKCOL / MOVE / COPY** | 写法五件 ✓ 覆盖语义呼应 PROPOSAL_OVERWRITE_UPLOAD |
| 记档 | **LOCK/UNLOCK 不支持**（405） | macOS/Linux/rclone 挂载不受影响 ✓ Windows 映射依赖锁 → 后续评估 |

## 3. 客户端兼容矩阵（预期）

| 客户端 | 只读 | 写 | 锁依赖 |
| --- | --- | --- | --- |
| rclone / davfs2 | ✓ | r103 | 无 ✓ |
| macOS Finder | ✓ | r103 | 无（宽松 ✓） |
| Cyberduck | ✓ | r103 | 无 ✓ |
| Windows 映射驱动器 | ✓ | r103 | **依赖锁** → 405 或后续评估 |

## 4. r103 前置侦察（2 击内 ✓）

`EntryRepo`/`BlobStore` trait 方法签名（list/get/read 形）→ server.rs 委托实现。

## 5. 门禁（Rust 面）

`cargo check -p vfiles-webdav` + `cargo test`（端到端 curl 实证并行 ✓）。
