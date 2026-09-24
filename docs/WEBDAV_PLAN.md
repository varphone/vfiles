# WebDAV 支持计划（RFC 4918 子集）

## 锁持久化更新

- exclusive write lock 已改为 SQLite 持久化，按命名空间与路径隔离；重启和同库多进程共享锁状态。
- SQLite 单语句 upsert 保证并发获取互斥；过期锁可被接管，刷新/释放均需匹配令牌。
- 验证覆盖独立应用实例间的锁可见性、写请求 423、并发单获锁及过期接管。

> 状态：**r102 架构定案 + crate 骨架**（本文件 = 实施契约 ✓ r103 = server 全写）。
> 目标：挂载入 Finder / Windows 映射驱动器 / rclone / Cyberduck / davfs2 通用生态。

## 0. 商业级清单定稿（r110'b ✓ 含缺口诚实注）

| 项 | 状态 | 注 |
| --- | --- | --- |
| OPTIONS / PROPFIND（Depth 0/1）/ GET / HEAD | ✅ **实装 + 真服务证**（curl 207 ✓） | GET 流式读取版本；PROPFIND 文件 `getlastmodified` 取当前版本时间，目录无版本时回退条目创建时间 |
| MKCOL / DELETE / MOVE | ✅ 实装（`WebdavWriteOps` ✓ 审计链 user_id ✓） | MOVE Overwrite T 在 SQLite 单事务内删除目标子树并改写源路径；blob 引用释放和快照在提交后处理 |
| **PUT** | ✅ **链实装**（`init_upload` + `complete_upload_from_stream` 流式直完 ✓） | bin 侧 `put_file` 转发 = 下段（签名已清 ✓） |
| COPY | ✅ **实装**（Destination + Overwrite + 锁前置 + 审计；文件复用 blob，目录递归复制） | `copy_entries` 提供 overwrite 和目标父目录检查；需持续做 RFC/客户端兼容验收 |
| LOCK / UNLOCK | ✅ exclusive write 实装（depth 0/infinity、refresh、SQLite 持久化、锁冲突原子判断、Second-N/Infinite） | 当前只广告并授予 exclusive；shared 明确 405。RFC 4918 允许服务器选择锁组合；共享锁互操作属于后续兼容性增强 |
| per-user ns | ✅ **实装**（`ensure_default_for_owner` ✓ 多用户隔离 ✓） |
| auth 门 | ✅ dispatch 顶部（Basic → verify → 401 + WWW-Authenticate ✓ OPTIONS 豁免 ✓） |
| 默认开启 | ✅ **用户令兑现**（`enabled: true` ✓ auth 强制防御 ✓ 真服务日志确证 ✓） |
| 边界/错误语义 | ✅ Depth infinity = 400 ✓ If 复杂式 = 412 记档 ✓ 锁冲突 = 423 ✓ token 不配 = 409 ✓ |
| **GET 流式化** | ✅ **r201-02 收口**（`get_stream` 直通 + ReaderStream ✓ **10MB sha256 一致性证** ✓ 内存爆除） |
| COPY | ✅ **已接线**（`WebdavWriteOps::copy_entry` → `DefaultWorkspaceService::copy_entries`；目标覆盖、子树保护、blob 复用与递归目录复制均有实现） |
| **台架缺口注** | cadaver 0.24 基础读写/目录/COPY/MOVE 由 `scripts/cadaver_probe.sh` 自动回归并接入 CI；rclone 1.60.1-DEV 已实测 MKCOL/PUT/list/GET；Windows 客户端与完整 litmus 套件仍待测 |

## 当前工作树补充（LOCK refresh 与请求 scope 校验）

- 空体 LOCK 识别为 refresh：从 `If` 头取唯一 `opaquelocktoken`，仅刷新同路径上仍有效的锁；成功返回原 token 与 lockdiscovery，失效 token 返回 412。
- 新 LOCK 解析 RFC `lockinfo` XML，只接受 `exclusive` + `write`；shared 请求明确返回 405，不再被静默授予 exclusive 锁。
- LOCK 接受 `Depth: 0` 与 `Depth: infinity`，省略时按 RFC 默认 `infinity`；祖先 infinity 锁会覆盖后代资源；直接冲突返回 423，infinity 锁遇到阻塞后代时返回含 423/424 的 207 Multi-Status。
- 写请求支持 RFC 4918 `If` 条件列表：列表内按 AND 求值、列表间按 OR 求值；支持未标记与 URI-tagged 列表，tagged 列表按挂载路径分别映射到 COPY/MOVE 源和目标；有锁写请求必须在匹配资源的成功列表中提供匹配的正向锁 token；ETag 与 `Not` 条件按当前实体状态求值，即使资源未锁也会校验。
- `If` 状态 token 接受任意合法 URI；未知 token 按“不匹配”参与条件求值，不会导致整个头部解析失败并屏蔽其它 OR 列表。`Not` 关键字按 ABNF 大小写不敏感解析。
- LOCK refresh 按同一套 `If` 条件解析执行，接受匹配当前资源的 URI-tagged 列表，并按实际资源的锁 token / ETag 求值。
- PUT 现在从 Axum `Body` 转为 `StreamReader` 直通 `complete_upload_from_stream_unknown_size`；不再先用 `usize::MAX` 将整个请求体复制到内存。流式计数沿用 HTTP 的较小文件上限，超过时返回 413，并清理上传会话与 blob 临时文件。
- DELETE、MOVE 源/目标与 COPY 覆盖目标会检查受影响子树中的活动锁；子项锁要求在 URI-tagged `If` 列表中按子项资源提交匹配 token，缺失返回 423、不匹配返回 412。
- PROPFIND / PROPPATCH 的 XML 请求体上限为 1 MiB；超过上限立即返回 413，不会把空体当作合法请求继续解析。

## 0.5 GET 流式化（r201 ✓ 商业级硬伤修 ✗ 大文件内存爆）

| 件 | 实装 |
| --- | --- |
| trait | `get_file(Vec)` → **`get_stream(ReadSeek, mime, size)`** ✓ |
| 服务 | **`open_file.reader` 直通**（底层 `get_blob_stream` 流式 API ✓ r103 已备） |
| 响应 | `tokio_util::io::ReaderStream` → `Body::from_stream`（tokio-util `io` feature ✓） |
| 实证 | **curl GET 200 / 19 bytes / 内容一致** ✓（免管道四 OK 链 ✓） |

## 1. 架构定案（r102 侦察实录）

| 面 | 定案 | 依据 |
| --- | --- | --- |
| 形态 | **独立 crate `vfiles-webdav`** + ✅ **共端口双轨（r-new）**：默认嵌入主端口 `/dav`（`nest_service` 异 state 挂入 ✗ nest 自剥前缀 ✓ 显式路由优先 fallback ✓）/ 显式 `VFILES_WEBDAV_PORT` = 独立端口现行为零回归回退（`VFILES_WEBDAV_MOUNT` 可改挂载点，空归一 `/dav` ✗ 根挂载不支持 = 方案核心冲突防御） | spike:axum0.8 `into_service` 一次过 ✓ |
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
| **r115（✓ 接入指引界面 ✨）** | **系统信息看板「WebDAV 接入」卡**（端点 URL（0.0.0.0 → 浏览主机名改写 ✓）+ 方法清单 + rclone/Finder/Windows 挂载示例折叠 ✓ 未启用态 = env 指引 ✓）= 商业产品同款（坚果云/Box 式）| 后端 system-info 扩 webdav 字段（`ConfigLoader::load` ✓ 首调误走 AppConfig 按错修 ✓）|
| **r110'c（✓ 读写全链真服务证 ✨）** | **GET/HEAD 实装**（`get_file` 门面 ✓ 404/头语义 ✓）+ **bin 转发**（open_file / init_upload+complete_upload_from_stream ✓）；**真服务六法链**：MKCOL/PUT/GET/HEAD/MOVE/DELETE 全绿 ✓ PROPFIND 文件类型判定正 | **修正**：init_upload(target=父目录, filename) 语义（误传全路径 = 建目录条目 ✗ PROPFIND collection 误标揭发 ✓）；curl -X HEAD 挂 = curl 怪癖（用 -I ✓）|
| **r109d（e2e 绿 ✓ fallback 盲区破）** | **e2e 启用全绿**（三断言真栈 ✓ = 可浏览挂载硬证（r107 遗留兑现 ✓））；**史诗级勘误** ✗✗✗：`.fallback(hello)`（r105 二分暂换**未还原**）= **r105-109 五轮 dispatch 全死码**（dbg-eprintln 一击破（断言级debug够不着 ✗））→ 还原 `.fallback(dav)` + hello 残骸清除；**根特判**（空 ns 无 root Entry ✗ 合成根响应（RFC 4918 ✓））| **工具语候选 #50：二分暂换当轮必还原（grep 验证式）** ✗✗ |
| **r109e（per-user ns ✓ 锁隔离 ✓）** | **per-user ns 实装**（dispatch `ensure_default_for_owner` 动态映射 ✓ 多用户隔离 ✓）；**锁表安全洞修** ✗✓（裸路径 key = 跨用户互锁 ✗ → **`ns:path` key 隔离**）；e2e 适配补验绿（1 passed ✓ 零回归） | PUT（init_upload 链）+ bin 挂载 + curl/rclone = **r110' 台架段** |
| **r110'a（✓ 可挂载真证 ✨）** | **bin 挂载全实**（WebdavWrite 转发 + build_webdav_runtime + 降级式 spawn ✓）；**curl 真服务三断言全绿**（OPTIONS 200+Allow ✓ 401 ✓ **207 multistatus XML** ✓）；**默认开启确证**（启动日志 ✓ 用户令兑现 ✓）| **三盲区破** ✗✗✗：`fallback(hello)` 残骸 + **spawn 顺序 bug**（`shutdown.wait_for` 在 `run` 前 = run 永不执行（r103 骨架 ✗）→ `tokio::select!` ✓）+ 旧二进制（**#51** `\| tail` SIGPIPE 假败 / **#52** Rust 改动 = `cargo build` 前置 ✓）|
| **r110'b 商业级清单定稿** | PUT（init_upload 链）+ 边界/错误语义 RFC 全检 + rclone/Windows 台架 + **商业级清单定稿**（含缺口注 ✓） | |
| **r110' 验收台架** | 边界/错误语义 RFC 全检 + curl/rclone/Windows 台架 + 商业级清单 | |
| r107（挂载段遗留） | bin 挂载 + curl/rclone e2e + GET（`EntryVersion.blob_id` ✓ 形已清） | 并入 r110' 台架 ✓ |
| 记档 | shared 锁明确不支持（405），`supportedlock` 只广告 exclusive write | RFC 允许锁能力子集；Windows/完整 litmus 互操作仍待实测 |

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
