# WebDAV RFC 4918 完备性基线（新目标作战地图 · round 1）

> 目标 = 支持完整协议规范（RFC 4918 core；版本控制 RFC 3253 / ACL 3744 /BIND 5842 不在本轮范围 ✗ 另立项再议）。每轮按 P0→P3 消项，状态随轮更新。

## 1. 方法面（§9-10）

| 方法 | 状态 | 缺口/备注 | 优先 |
| --- | --- | --- | --- |
| OPTIONS | ✅ r214 修正 | Allow 全 10 方法 + `DAV: 1, 2` 实证 ✓ | - |
| PROPFIND | ✅ **r2 五式实证** | 请求体解析落（roxmltree 选型 ✗ allprop/prop/propname ✓ 未支持属性 = 404 propstat ✓ 非法 400 ✓）| - |
| （同上）children | ✅ **r3 债还清** | 双值齐（**length + contenttype** ✓ video/mp4/5B 实证 ✓ 集合不带 ✓）⚠️ 每文件 get_stream = **0.3s/50 文件（6ms/个）可见延迟 → 批量 SQL 优化 P1 立债** | P1性能 |
| PROPFIND Depth | ✅ **r2 顺手落** | infinity = **403** 合规码 ✓ 实证 | - |
| GET / HEAD | ✅ r201+r211 | 流式 + Range 206/416/Accept-Ranges 全实证 ✓ | - |
| PUT | ✅ r110'b | 流式完成链 + 409 日志 ✓ | - |
| DELETE | ✅ r108' | 递归 + 409 ✓ | - |
| MKCOL | ✅ r110' | 201/409 ✓ | - |
| MOVE | ✅ r108' | overwite 语义简式 ✓ | - |
| **COPY** | ❌ **501** | **P0 核心缺口**（深域件：blob 复用 + entry 复制 + 审计链，既有排期细化 ✓）| **P0** |
| **PROPPATCH** | ❌ **零实现** | **P0 核心缺口**（propertyupdate XML set/remove ✗ 需 XML 解析）| **P0** |
| LOCK / UNLOCK | ✅ r109a | 无限期锁表 + 423 ✓；**Timeout 有限支持 / 锁刷新 = 缺** | P1 |
| POST | ➖ | RFC 无定义（405 ✓ 合规）| - |

## 2. 请求头/状态码面

| 项 | 状态 | 备注 | 优先 |
| --- | --- | --- | --- |
| Depth 头 | ✅ | 0/1 ✓ infinity 见上 | P1 |
| If 头（锁/条件） | ❌ | **条件请求零实现**（If-Match/If-None-Match/If ✗ 写并发安全核心）| **P0** |
| 412 Precondition | ❌ | 随 If 头一起做 | **P0** |
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
4. **children get_stream 批量 SQL**（r3 立债 ✗ 6ms/文件 → 条 JOIN 消 N+1）← 下轮首项
5. **COPY 方法**（P0 核心：blob 复用 + entry 复制 + 审计链）
6. **PROPPATCH 方法**（P0 核心：propertyupdate set/remove）
7. **If 头 + 412**（P0 并发安全）

> 记录纪律：每轮改协议面 = 同轮 curl 实证行入表；用户日志线索（gvfs/VLC）= 一等证据源。
