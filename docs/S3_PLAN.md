# S3 兼容 API（r2 九式 9/9 → **r3 列表/元数据/Range 八式 + 真 AWS SDK 9/9**）

## 状态（r3 末 · 列表面商业级完备 = 自写探针 17/17 + boto3 9/9）

- **新增能力（`crates/vfiles-s3/src/lib.rs`）**：
  | 面 | 语义 |
  | --- | --- |
  | `ListObjectsV2` | prefix · **delimiter→CommonPrefixes**（只折一层）· **continuation-token** · start-after · **max-keys**（尊重请求、上限 1000）· is-truncated · **next-continuation-token** · key-count |
  | `ListObjects`(V1) | marker · delimiter · max-keys · next-marker · common-prefixes |
  | `Object` 元数据 | **size / last-modified / ETag**（版本 id hex，与 WebDAV/S3 GET 同式） |
  | `GetObject`/`HeadObject` | **last-modified** + **HTTP Range → 206/Content-Range**（s3s 见 content_range 自置 206 ✗ `Range::check` 夹取 + 416 InvalidRange） |
  | bucket | `default` → `<?xml ...><ListBucketResult>` |
- **关键实现**：`collect_objects` 一次树遍历取 `(key,size,mtime,etag)` → `build_entries`（prefix 过滤 +
  delimiter 折叠去重 + 排序）→ `paginate`（`after` 独占切片 ✗ token = 页尾 key = 无重无漏）·
  `resolve_max_keys`（负值 InvalidArgument / 上限 1000）· `resolve_range`（`Range::check`）。
  纯函数 6 项单测（delimiter 折一层 / 分页续页 / 满页不截断 / max-keys 规则 / range 四态）。
- **实证（入仓双通道可复演）**：
  - `scripts/sigv4_probe.py`（自写 SigV4）= **17/17 PASS**：r2 九式 + 元数据 / delimiter /
    分页 12a+12b（无重无漏）/ V1 / Range 206 / Range 416 / HEAD Last-Modified
  - `scripts/boto_probe.py`（**真 AWS SDK** = botocore 签名+XML+**paginator**+Range）= **9/9 PASS**：
    含 SDK 驱动 `get_paginator("list_objects_v2")` PageSize=1 = 4 页 4 key 无重
- **扩展债（更新）**：put 流式直连 · multipart upload · **`DeleteObjects` 批量删** · per-user 凭证 ·
  region 校验 · 大桶 SQL 分页（现全量枚举后内存分页 = 十万级需换）。

## 形态

| 项 | 决策 | 依据 |
| --- | --- | --- |
| 端口 | **专用 9000**（`VFILES_S3_PORT`） | path-style `/bucket/key` 与前端根语义冲突 ✗ MinIO 惯例直觉 ✓ |
| 桶 | 单虚拟 **`default`**，严格语义（他桶 → NoSuchBucket 404 XML） | 客户端列桶自配对（ListBuckets=[default] ✓） |
| 认证 | **SigV4 静态单对**（`VFILES_S3_ACCESS_KEY/SECRET_KEY`，缺省 uuid 随机 + warn 打印 access，secret 不落日志） | s3s 内建验签 + `EnvAuth` 十五行自写（SimpleAuth 也 pub 可换） |
| 启用 | **默认关**，`VFILES_S3_ENABLED=true` 显式开 | 新协议面显式启用原则（关时日志明示启用法） |
| 实现 | crate `vfiles-s3`：`VfilesS3` 六方法 override（trait 114 全默认 NotImplemented） | 薄组装 ✗ 写面 = WebDAV 同源 app 层链（init+complete/Cursor） |
| key 映射 | 默认 ns 根相对路径（`collect_keys` 递归展平；排序截 1000） | 树 = flat keys |
| ETag | **version-id hex 裸值 + `ETag::Strong` 包裹**（to_http_header 自附引号） | **与 WebDAV derive_etag 跨协议同式** ✗ 引号单所有者 = s3s header 层 |
| DELETE | 幂等 204（NotFound 吞） | S3 语义 |

## 九式实证（`crates/vfiles-s3/scripts/sigv4_probe.py` 入仓可复演）

```
1 ListBuckets default        200
2 ListObjectsV2              200
3 PUT 200 +version           v0->1        ← 版本联动（db 断言）
4 PUT same key +version      v1->2        ← 同名多版本 = 产品核心跨协议实证 ✓✓
5 GET content+ETag           200 etag="bfc3a89b0d004f0c94eaee506872357b"
6 HEAD CL                    200 cl=17
7 DELETE idempotent          204/204
8 NoSuchBucket               404 XML
9 wrong secret               403
== 9/9 PASS ==
```

复演：起服（四 env）后
`python3 crates/vfiles-s3/scripts/sigv4_probe.py http://127.0.0.1:9000 <access> <secret> <db路径>`

## 启用与接入

```bash
export VFILES_S3_ENABLED=true          # 默认关 ✗ 显式开
export VFILES_S3_ACCESS_KEY=AKIAVFILES0001   # 缺省 = 随机 + warn
export VFILES_S3_SECRET_KEY=...              # 缺省 = 随机
# endpoint = http://<host>:9000   bucket = default   path-style
# 客户端：rclone s3 / Cyberduck / AWS CLI / mountpoint-for-s3 全通
# 通用字段/值：--provider=MinIO --endpoint=http://host:9000 --region 任意
```

## 扩展债（记档待排期）

1. **per-user 凭证**（access/secret 表 + 管理端点，替换全局单对）
2. **ListObjects 续页**（continuation_token；r1 全量截 1000）
3. **delimiter=CommonPrefixes**（目录组前缀折叠；r1 忽略 delimiter 递归全列）
4. **put 流式直连**（r1 聚合 Vec；与 WebDAV GET 流式同级优化 ✗ complete_upload_from_stream 已是流式形状，只差 body 不落地）
5. **last_modified/creation_date**（r1 None；接版本时间 = entry_versions.created_at）
6. **multipart upload**（大对象分片；r1 单请求）
7. S3 面的 /health 路由（现仅 fallback）
8. region 校验（现任意 region 通过 = SigV4 scope 内一致即验）

## 探针 saga（r2 · 签名真容剥七层）

1. **`SignatureDoesNotMatch` ×9** → 服务日志三行权威 ERROR = 验签在跑 ✗ 真因 = heredoc `"\\n".join` **字面双反斜杠**（canonical 拼接全废）→ `chr(92)` 零转义修（修脚本自身也要防转义地狱）
2. **404 簇（式3-7）** → **path-style bucket 段缺**（探针把 key 当 path ✗ 正确 = `/default/key`；式8 `/wrongbucket` 404 反证 bucket 段在工作 =探测面互证）
3. **式5 etag=None** → r1 设计写了 ETag 方案、impl 漏赋字段（设计≠实现的检查缺位）→ 补 `find_by_path` 双查 + `e_tag`（真名非 etag，编译器 similar help 带路）
4. **`Option<Entry>` 未解包** + **`ETag` 是枚举**（`Strong/Weak`，引号由 `to_http_header` 自附 = 我存裸 hex 防双引）
5. **式9 假阳警示**：1-8 全 403 时式9 也 PASS = **探针前置不自足**（#60 探针形态再证：断言需依赖上游成真）
6. 装配侧 saga：watch 定义与 AppState move 的依赖序矛盾（定义前移解）+ Cargo 键名连字符（`vfiles-s3` key vs `vfiles_s3` 引用 = help 直给）

门禁：`cargo check / --tests / test / build --bin` 四门全绿（24 suites）✗ **#61 纪律 r2 首战全程三跑护航**。
