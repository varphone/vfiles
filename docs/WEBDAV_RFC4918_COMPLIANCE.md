# WebDAV RFC 4918 完备性基线（新目标作战地图 · round 1）

> 目标 = 支持完整协议规范（RFC 4918 core；版本控制 RFC 3253 / ACL 3744 /BIND 5842 不在本轮范围 ✗ 另立项再议）。每轮按 P0→P3 消项，状态随轮更新。

## 1. 方法面（§9-10）

| 方法 | 状态 | 缺口/备注 | 优先 |
| --- | --- | --- | --- |
| OPTIONS | ✅ r214 修正 | Allow 全 10 方法 + `DAV: 1, 2` 实证 ✓ | - |
| PROPFIND | ✅ **r2 五式实证** | 请求体解析落（roxmltree 选型 ✗ allprop/prop/propname ✓ 未支持属性 = 404 propstat ✓ 非法 400 ✓）| - |
| （同上）children | ✅ **r3 债还清** | 双值齐（**length + contenttype** ✓ video/mp4/5B 实证 ✓ 集合不带 ✓）✅ **r4 批量 SQL 落**（一条字面 SQL + 2 标量子查询 ✗ **51 行实测 0.001s** ✓；⚠️ 真性能头 = **认证哈希 0.25s/请求 → 热验缓存 P1 新债**）| 认证P1 |
| PROPFIND Depth | ✅ **r2 顺手落** | infinity = **403** 合规码 ✓ 实证 | - |
| GET / HEAD | ✅ r201+r211 | 流式 + Range 206/416/Accept-Ranges 全实证 ✓ | - |
| PUT | ✅ r110'b | 流式完成链 + 409 日志 ✓ | - |
| DELETE | ✅ r108' | 递归 + 409 ✓ | - |
| MKCOL | ✅ r110' | 201/409 ✓ | - |
| MOVE | ✅ r108' | overwite 语义简式 ✓ | - |
| **COPY** | ✅ **r5 八式实证** | 递归子树 + blob 零字节复用（create_version upsert ref++ ✓）+ **审计落表** ✓；dst存在/自复制/自子树 = 409 ✓ dest缺 = 400 ✓ Allow 入 ✓ **Overwrite T 完整覆盖（删旧+blob release）= P1 记债** | P1覆盖 |
| **PROPPATCH** | ✅ **r6 七式实证** | propertyupdate 解析（roxmltree 按序 ✓）+ 每操作 propstat 200/403 ✓ **displayname set = 真改名**（move 单源直路径复用 ✓ PROPFIND 新名可见 ✓）+ 审计落表 ✓ Allow 入 ✓ **自定义属性 k/v 持久化 = P1 记债** | P1属性存储 |
| LOCK / UNLOCK | ✅ r109a | 无限期锁表 + 423 ✓；**Timeout 有限支持 / 锁刷新 = 缺** | P1 |
| POST | ➖ | RFC 无定义（405 ✓ 合规）| - |

## 2. 请求头/状态码面

| 项 | 状态 | 备注 | 优先 |
| --- | --- | --- | --- |
| Depth 头 | ✅ | 0/1 ✓ infinity 见上 | P1 |
| If 头（锁条件） | ✅ **r7 七式实证** | lock-token 形解析 + **423/412 分码**（RFC §9.10.6 ✓）+ 三臂缺口补齐（PUT/COPY/PROPPATCH 此前零锁检 = 锁摆设 ×3 ✗ 源/目标双查 ✓）**If-Match/ETag 面 = P1 记债**（无 ETag 面）| P1 ETag |
| 412 Precondition | ✅ **r7** | 有 If 不匹配 = 412 / 无 If 锁住 = 423 分码纯函数 + 单测 ×5 ✓ | - |
| Timeout 头（LOCK） | ❌ | 只 Infinite（Second-N 不解析 ✗ 客户端常发）| P1 |
| 423 Locked | ✅ | ✓ | - |
| 405 Method Not Allowed | ✅ | default 分支 ✓ | - |
| 401 + WWW-Authenticate | ✅ r106 | ✓（失败 WARN 日志 r205）| - |
| 409 Conflict | ✅ | + WARN 日志 r207 ✓ | - |
| 416 Range | ✅ r211 | ✓ 七式实证 | - |
| If-None-Match on GET | ❌ | 缓存语义（ETag 面联动）| P2 |
| Destination/Overwrite/Depth on MOVE| ✅ r108' | ✓ | - |

## 3. 资源属性面（PROPFIND 响应）

| 属性 | 状态 | 备注 | 优先 |
| --- | --- | --- | --- |
| resourcetype | ✅ r209 | 文件/目录精确 ✓ | - |
| displayname | ✅ | ✓ | - |
| getlastmodified | ✅ | ✓ | - |
| **getcontentlength 单目标** | ✅ r213 | ✓ 三属性同框实证 | - |
| **getcontentlength children** | ❌ | 批量（P0 债 = r213 记档）| **P0** |
| getcontenttype | ✅ **r3 顺车** | 单目标 + children 双位出 ✓ 检测/扩展名回退 ✓ | - |
| getetag | ❌ | ETag 面前置（If-Match 联动）| P1 |
| creationdate | ❌ | entry.created_at 可出 | P2 |
| lockdiscovery / supportedlock | ❌ | LOCK 属性面（随 P1 锁补全）| P1 |
| owner / displayname 父链 | ❌ | 简式 owner = 认证用户名 | P2 |

## 4. 结构/语义面

| 项 | 状态 | 备注 | 优先 |
| --- | --- | --- | --- |
| 207 multistatus | ✅ | ✓ | - |
| allprop 固定集 | ✅ | 但**请求体解析缺**（见方法面 P0）| P0 |
| href 编码/斜杠 | ✅ r204/209 | 全绿实证 ✓ | - |
| percent-decode 全入口 | ✅ r204 | 5 入口 + Destination ✓ | - |
| 流式 GET/PUT 内存安全 | ✅ r201/110'b | 10MB/分段实证 ✓ | - |
| 排障观测（访问/认证/失败/信号）| ✅ r205-210 | 全链 info 级 ✓ 降噪三式 ✓ | - |
| XML 解析依赖 | ✅ **r2 选型定** | **roxmltree v0.21.1**（KB 级 DOM / 命名空间原生 / 零依赖 ✗ PROPPATCH 复用 ✓）| - |

## 5. 消项节奏（P0 栈 = 下轮起）

1. ~~XML 选型~~ ✅ r2（roxmltree）
2. ~~PROPFIND 请求体解析~~ ✅ r2（五式 + 404 propstat + infinity-403 顺手）
3. **children getcontentlength 批量**（P0 债，客户端列目录必需 ← 下轮首项）
4. ~~children 批量 SQL~~ ✅ r4（0.001s 实测 ✓）
4a. **认证热验缓存**（0.25s/请求真头 ✗ r4 分层发现）← P1 新债
5. ~~COPY~~ ✅ r5（八式 + 审计 ✓）
5a. **审计全线写入**（r5 发现：AuditService.record 生产零调用 ✗✗ 看板恒空真相 ✗
    copy 已接 = 结构就位 → 登录/管理/其余 WebDAV 写点批量接）← **P0 新债**
5b. Overwrite T 完整覆盖（删旧 + blob release 链）← P1
6. ~~PROPPATCH~~ ✅ r6（七式 + 审计 ✓）
7. ~~If 头 + 412~~ ✅ r7（七式 + 分码 + 三臂补锁检 ✓）
8. **审计全线写入**（登录/管理/其余写点 → record 批量接）← **P0 末项** ← 下轮首项（结构双示范 ✓）

> 记录纪律：每轮改协议面 = 同轮 curl 实证行入表；用户日志线索（gvfs/VLC）= 一等证据源。
