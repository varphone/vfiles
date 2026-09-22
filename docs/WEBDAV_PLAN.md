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
| **r104（三实件 + 谜案记）** | `auth::verify` 注入式（4 测试绿 ✓）；**PROPFIND 体全写**（find_by_path/find_children → multistatus ✓ Depth 0/1）；OPTIONS/405 契约实 | dispatch 501：**axum E0277 Handler 谜案**（最小 handler 二分 = 体内问题 ✓ debug_handler/官方 example 对照 = r105 一击破） |
| **r105（谜案破 ✓ 接线完）** | **E0277 真因 = `&Request` 跨 await 非 Send**（debug_handler 全窗破案 ✓ 教科书 Send 修复式 = 同步提取拥有值 ✓）；**PROPFIND 接线完**（propfind_owned → multistatus ✓ Depth 0/1）；4 测试绿 | bin/config 挂载 = r106 机械段（build_ftp_runtime 同式清单 ✓） |
| **r106（安全段 ✓ 债清）** | **dispatch 顶部安全门**（Basic→verify 回调→401+WWW-Authenticate ✓ OPTIONS 豁免（RFC 无泄露））；`VerifyFn` 型 = bin 接 `verify_credentials` ✓ | 首版门藏 PROPFIND 分支 = GET 裸奔洞自察上移 ✓ |
| **r108'（写面三件 ✓ 商业级一段）** | **MKCOL/DELETE/MOVE 实装**（`WebdavWriteOps` dyn-trait ✓ 审计链 user_id ✓ VerifyFn 升级回 User ✓）；MOVE `Destination` 解析（纯函数单测 ✓ 挂载点 = root 记档）；**PUT** = init_upload 链 → r109'；**COPY** = 501 记档（rclone GET+PUT 不依赖 ✓） | 5 测试绿 ✓ **#46**：write_op(&req) 坑二号 ✗ 纯拥有参式贯彻 |
| **r109' 商业级核心** | **LOCK/UNLOCK**（Windows 映射依赖 ✓ 必做）+ per-user ns + **默认开启 config**（auth 强制防御 = FTP bail! 同款）+ PUT（init_upload 链） | |
| **r110' 验收台架** | 边界/错误语义 RFC 全检 + curl/rclone/Windows 台架 + 商业级清单 | |
| r107（挂载段遗留） | bin 挂载 + curl/rclone e2e + GET（`EntryVersion.blob_id` ✓ 形已清） | 并入 r110' 台架 ✓ |
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
