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
| DELETE | ✅ **r11 顺修** | 递归 + 409 ✓ **成功 = 204**（原三 op 全 201 = RFC 违背顺手修 ✓ 实证）| - |
| MKCOL | ✅ r110' | 201/409 ✓ | - |
| MOVE | ✅ **r11/r12 覆盖语义；原子替换补强** | 201 新建 / **412**（F+存在）/ **204**（T + 缺省=T）✓；Overwrite T 目标子树删除与源路径更新在 EntryRepo 单 SQLite 事务中提交，事务冲突回滚完整；提交后释放 blob 引用并记录删除/改名快照 | 客户端矩阵继续扩充 |
| **COPY** | ✅ **r5 八式实证** | 递归子树 + blob 零字节复用（create_version upsert ref++ ✓）+ **审计落表** ✓；dst存在/自复制/自子树 = 409 ✓ dest缺 = 400 ✓ Allow 入 ✓ **Overwrite 全语义 ✅ r10**（T=删旧重建 204 ✓ F=412 ✓ 缺省=T ✓ 目录覆盖 204+旧消失 ✓ 审计 ✓）| - |
| **PROPPATCH** | ✅ **r6 七式实证** | propertyupdate 解析（roxmltree 按文档顺序 ✓）+ 每操作 propstat 200/403 ✓ **displayname set = 真改名** ✓ + 自定义属性持久化 ✓；属性名使用 namespace URI + local-name，旧键迁移保留原 DAV 命名空间响应 | - |
| LOCK / UNLOCK | ✅ r109a | 无限期锁表 + 423 ✓；**Timeout 有限支持 / 锁刷新 = 缺** | P1 |
| POST | ➖ | RFC 无定义（405 ✓ 合规）| - |

## 2. 请求头/状态码面

| 项 | 状态 | 备注 | 优先 |
| --- | --- | --- | --- |
| Depth 头 | ✅ | 0/1 ✓ infinity 见上 | P1 |
| If 头（锁条件） | ✅ **r7 七式实证** | lock-token 形解析 + **423/412 分码**（RFC §9.10.6 ✓）+ 三臂缺口补齐（PUT/COPY/PROPPATCH 此前零锁检 = 锁摆设 ×3 ✗ 源/目标双查 ✓）**If-Match/ETag 面 = P1 记债**（无 ETag 面）| P1 ETag |
| 412 Precondition | ✅ **r7** | 有 If 不匹配 = 412 / 无 If 锁住 = 423 分码纯函数 + 单测 ×5 ✓ | - |
| Timeout 头（LOCK） | ✅ **r15 七式实证** | `Second-N` 解析（多值取首个可解析 ✓ 纯函数单测）+ 响应回显（Second-N/Infinite）+ **惰性过期**（blocked/lock/unlock 三路径 ✗ 到期释放 201 实证 ✓ 过期 unlock 409 / 重锁 200 接管 ✓ 零后台任务 ✓）| - |
| 423 Locked | ✅ | ✓ | - |
| 405 Method Not Allowed | ✅ | default 分支 ✓ | - |
| 401 + WWW-Authenticate | ✅ r106 | ✓（失败 WARN 日志 r205）| - |
| 409 Conflict | ✅ | + WARN 日志 r207 ✓ | - |
| 416 Range | ✅ r211 | ✓ 七式实证 | - |
| If-None-Match on GET | ✅ **r14** | `*`/列表/精确 三态（单测 ×2 ✓）命中 **304 + ETag 头** / 未命中 200 ✗ 写后值变 = 200 实证 ✓ | - |
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
| getetag | ✅ **r14 九式实证** | = current_version_id 派生 `"hex32"`（r4 SQL 零新查询 ✓ 强 ETag 引号式 ✓ XML `&quot;` 转义合法 ✓ None = 目录跳过 ✓ 支持集 5→6 + readonly +1 ✓）| - |
| creationdate | ✅ **r16 七式实证** | RFC 3339 ISO（≠ lastmod 的 RFC1123 ✓ 恒有值 ✓ 根 = 合成记档（lastmod 同式）✓ children 批量 ✓ readonly 不可写 ✓）| - |
| lockdiscovery / supportedlock | ✅（共端口/锁属性补齐轮） | PROPFIND allprop/prop/propname 均声明；活动锁按 ns+路径返回 token/owner/remaining timeout，无锁返回空 `lockdiscovery`；两属性 PROPPATCH set/remove 均拒绝 | - |
| owner | ✅ **r16** | **r109e 隔离恒等式**（ns owner ≡ 认证者 = 零查询白捡 ✓ 记档：共享 ns 启用时回查 namespaces.owner_user_id（真值源 0001:30 已在）✓ readonly 不可写 ✓）| - |

## 4. 结构/语义面

| 项 | 状态 | 备注 | 优先 |
| --- | --- | --- | --- |
| 207 multistatus | ✅ | ✓ | - |
| allprop 固定集 | ✅ | 但**请求体解析缺**（见方法面 P0）| P0 |
| href 编码/斜杠 | ✅ r204/209 | 全绿实证 ✓ | - |
| percent-decode 全入口 | ✅ r204 | 5 入口 + Destination ✓ | - |
| 流式 GET/PUT 内存安全 | ✅ r201/110'b | 10MB/分段实证 ✓ | - |
| 排障观测（访问/认证/失败/信号）| ✅ r205-210 | 全链 info 级 ✓ 降噪三式 ✓ | - |
| **审计全线（WebDAV）** | ✅ **r8 八写点收官** | `audit_write` 统一 helper（COPY/PROPPATCH 十行式收口）+ put/mkcol/delete/move/lock/unlock 六臂包裹 ✗ **七 action 落表实证** ✓ 失败面 = warn 日志（表记 Failure = P1）| P1失败面 |
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
8. ~~审计全线（WebDAV 八写点）~~ ✅ r8（七 action 落表实证 ✓）
   - **跨目标债**：HTTP 登录/管理面审计（record 在那里也是零调用 ✗ 看板恒空的另一半
     真相）= **新目标边界外**（记入 OPTIMIZATION 线 ✗ 本目标 = 协议完备性）

### 🏆 P0 栈全清（r8 · 方法/头/属性/结构四面 P0 全数落地）

✅ XML 选型 / PROPFIND 解析+404propstat+403 / children 双值+批量 / COPY 八式 /
PROPPATCH 七式 / If+412 七式 / 审计八写点 / 声明面（Allow+DAV class）

**P1 进度**：
- ✅ r9 **认证热验缓存**（SHA-256 cred key 原文零存 ✗ 只缓存成功（爆破防护零损 =
  错密码双发 0.244/0.250s 实证 ✓）✗ disabled 每命中实时查 = 禁用即时 ✓ Clone = Arc
  共享免疫分叉 ✓ **分层计时：0.328s → 1.6ms ≈ 200×** ✗ 改密码 30s 窗 = 记档权衡）
- ✅ r10 **COPY Overwrite 全语义**（六式实证 ✗ 删旧 = delete_entries 全链自带 blob
  release ✓ 响应码 T=204/F=412/缺省=T 按 RFC §9.3.3 修正 r5 的全 409 违背 ✓）
- ⚠️ r10 探针：**MOVE dst 存在 = 409（Overwrite 未接）** = 同语义缺口 → 下轮首项顺接
- ✅ r11 **MOVE Overwrite 文件面**（412/204/缺省 T 三码实证 + 内容变 ✓）+ **DELETE 204
  顺修**（RFC 违背第二处清）+ ⚠️ **同名目录覆盖 = 结构债**（move 母版 join 语义级 ✗
  3 外调共享 → 三面评估 = P1 栈新序首位）
- ✅ r12 **同名目录覆盖**（三面评估 → dest 语义参数化 ✗ 四调用点如实标（2 完整路径
  / 2 容器）✗ 多源强制容器 ✓ 母版语义级精修一处判定三分支 ✗ **P1 栈首位清**）
- ✅ r13 **ETag+If-Match / LOCK Timeout / 属性面 getetag/creationdate/owner / 失败面审计表记 / HTTP 审计跨目标债（原自定义条已清）属性 k/v**（七式实证 ✓ 属性持久化全语义）+ ⚠️ **r12 http 判定勘误**
  （move_route 测试真形 = to 完整路径 → tree 传 Path ✗ 4306 = 唯一 container ✓
  **测试真形定案纪律** ✗ 判定类须有测试/实证背书）
**P1 进度（r13-r15 三清 ✗ 清单现状）**：
- ✅ r13 自定义属性 k/v（七式）→ ✅ r14 ETag+If-Match（九式）→ ✅ r15 LOCK Timeout（七式）
- ✅ r16 **creationdate + owner**（七式 + 恒等洞察 ✓ **属性面 P1 清**）
**🏆 P1 协议域全清（r17）**：失败面审计表记 ✅（409/5xx → Failure 落表实证 ✗ 423/412 = warn 覆盖注 ✗ PROPPATCH = 请求处理语义注）/ HTTP 审计 = **跨目标池**（非协议面）
- 基线 P2/P3 = **零命中**（grep 权威）→ **RFC 4918 core 四面达成**（残余 = 范围外记档清单本表各处 ✗ 实证索引 = OPTIMIZATION 4.149-4.193）
属性 k/v / ETag+If-Match 面 / LOCK Timeout 有限 / getetag+creationdate 属性 /
失败面审计表记 / HTTP 审计跨目标债

> 记录纪律：每轮改协议面 = 同轮 curl 实证行入表；用户日志线索（gvfs/VLC）= 一等证据源。
