# WebDAV 支持计划（RFC 4918 子集）

> 状态：**r102 架构定案 + crate 骨架**（本文件 = 实施契约 ✓ r103 = server 全写）。
> 目标：挂载入 Finder / Windows 映射驱动器 / rclone / Cyberduck / davfs2 通用生态。

## 0. 商业级清单定稿（r110'b ✓ 含缺口诚实注）

| 项 | 状态 | 注 |
| --- | --- | --- |
| OPTIONS / PROPFIND（Depth 0/1）/ GET / HEAD | ✅ **实装 + 真服务证**（curl 207 ✓） | GET = 版本链（`EntryVersion.blob_id` 形清 ✓ 接线待接） |
| MKCOL / DELETE / MOVE | ✅ 实装（`WebdavWriteOps` ✓ 审计链 user_id ✓） | |
| **PUT** | ✅ **链实装**（`init_upload` + `complete_upload_from_stream` 流式直完 ✓） | bin 侧 `put_file` 转发 = 下段（签名已清 ✓） |
| COPY | ⚠️ **501 记档** | 无后端 copy API ✓ rclone GET+PUT 不依赖 ✓ |
| LOCK / UNLOCK | ✅ 实装（exclusive / depth 0 ✓ `ns:path` 隔离 ✓） | timeout = Infinite 记档；shared lock = 不支持（405 ✓） |
| per-user ns | ✅ **实装**（`ensure_default_for_owner` ✓ 多用户隔离 ✓） |
| auth 门 | ✅ dispatch 顶部（Basic → verify → 401 + WWW-Authenticate ✓ OPTIONS 豁免 ✓） |
| 默认开启 | ✅ **用户令兑现**（`enabled: true` ✓ auth 强制防御 ✓ 真服务日志确证 ✓） |
| 边界/错误语义 | ✅ Depth infinity = 400 ✓ If 复杂式 = 412 记档 ✓ 锁冲突 = 423 ✓ token 不配 = 409 ✓ |
| **台架缺口注** | ⚠️ **rclone/Windows 客户端台架** = 待装验（curl 三断言已证栈级 ✓）；bin put_file 转发 = 下段一击（签名全清 ✓） |

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
| **r109a（锁核 ✓ 完成）** | **LOCK/UNLOCK 实装**（内存锁表 ✓ `opaquelocktoken:` 零依赖铸造 ✓ exclusive/depth0 ✓ lockdiscovery §14.13 ✓）；**写操作 423 校验**（`If` token 放行 ✓ 多重式拒 412 记档 ✓）| 9 测试绿 ✓ **#46 三号实录**（lock_op(&req) 同坑 → 纯拥有参式 = **编码模板纪律**）|
| **r109b（config 段 ✓ 默认开令兑现）** | **`WebdavConfig`（enabled 默认 true ⚠️ 用户令）**+ `resolve_webdav_enabled` 六分支（**语义差** = `(None,false)→Err`（FTP 软停 ✗ WebDAV 严格 = 默认开+auth 强制双保 ✓））+ env 三键 + std env 直取（零猜式 ✓）| 18 config 测试绿 ✓ **#49**：`ftp,` 简写字段 = grep 冒号盲区（三轮假"零 literal"）+ **#14 三犯**（错误窗连环截 ✗ 无窗直读式立）|
| **r109c（e2e 起草 ✓ 封存止损）** | e2e 集成测起草（真 sqlite + tower oneshot + 三断言 ✓ 范本 = ftp_end_to_end Harness ✓）；**排障链 14 错止损**（存构造 arity 战/Arc-dyn/RateLimitPolicy 字段/AuthService concrete 3参/register 真名 ✗✗）→ **feature 门封存**（`--features e2e` 启 ✓ 剩 2 错 = RegisterRequest import（`vfiles_domain::types` ✓ 已定位）+ Workspace 4 参 concrete 按错式） | **意外收获** ✨：`ensure_default_for_owner(&UserId)` = **per-user ns 映射真身**（r109d 形提前明 ✓）|
| **r109d（e2e 绿 ✓ fallback 盲区破）** | **e2e 启用全绿**（三断言真栈 ✓ = 可浏览挂载硬证（r107 遗留兑现 ✓））；**史诗级勘误** ✗✗✗：`.fallback(hello)`（r105 二分暂换**未还原**）= **r105-109 五轮 dispatch 全死码**（dbg-eprintln 一击破（断言级debug够不着 ✗））→ 还原 `.fallback(dav)` + hello 残骸清除；**根特判**（空 ns 无 root Entry ✗ 合成根响应（RFC 4918 ✓））| **工具语候选 #50：二分暂换当轮必还原（grep 验证式）** ✗✗ |
| **r109e（per-user ns ✓ 锁隔离 ✓）** | **per-user ns 实装**（dispatch `ensure_default_for_owner` 动态映射 ✓ 多用户隔离 ✓）；**锁表安全洞修** ✗✓（裸路径 key = 跨用户互锁 ✗ → **`ns:path` key 隔离**）；e2e 适配补验绿（1 passed ✓ 零回归） | PUT（init_upload 链）+ bin 挂载 + curl/rclone = **r110' 台架段** |
| **r110'a（✓ 可挂载真证 ✨）** | **bin 挂载全实**（WebdavWrite 转发 + build_webdav_runtime + 降级式 spawn ✓）；**curl 真服务三断言全绿**（OPTIONS 200+Allow ✓ 401 ✓ **207 multistatus XML** ✓）；**默认开启确证**（启动日志 ✓ 用户令兑现 ✓）| **三盲区破** ✗✗✗：`fallback(hello)` 残骸 + **spawn 顺序 bug**（`shutdown.wait_for` 在 `run` 前 = run 永不执行（r103 骨架 ✗）→ `tokio::select!` ✓）+ 旧二进制（**#51** `\| tail` SIGPIPE 假败 / **#52** Rust 改动 = `cargo build` 前置 ✓）|
| **r110'b 商业级清单定稿** | PUT（init_upload 链）+ 边界/错误语义 RFC 全检 + rclone/Windows 台架 + **商业级清单定稿**（含缺口注 ✓） | |
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
