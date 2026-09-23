# S3 兼容 API（… r29 逐键条件 → r30 条件读 304 → **r31 逐键时间/大小条件**）

## 状态（r31 末 · 批量删条件面收口 = ETag + 时间 + 大小）

- **实装**：`DeleteObjects` 逐键条件扩到**三式**——`ETag`（r29）· **`LastModifiedTime`** · **`Size`**；
  任一不符 → 该键 `PreconditionFailed`（不拖累整批）。`size` 经**父目录 `children_with_meta` 缓存**
  取版本大小（同目录多键只查一次）。
- **真机抓到的坑**：库内 `created_at` 是高精度时间，而客户端往返的 `LastModifiedTime` 只到**秒** →
  直接比较必判不等；改为经 **HTTP-date 归一到秒**后比较（`same_second`）。
- **真机验收（真 SDK，6 项）**：时间+大小全对者删 / 只大小对者删 ✓ · `size` 不符 412 ✓ · 时间不符 412 ✓ ·
  被拒者仍在 ✓ · 已删者消失 ✓ · 只带正确时间重试成功 ✓。
- **回归**：自写探针 **24/24** · 真 SDK **42/42**（+时间/大小条件）。
- **仍债**：`ListObjectVersions` / `PutBucketVersioning`（真版本控制）· region 校验（有意不做）。

## 状态（r30 末 · 条件读 = HTTP 缓存语义）

- **实装**：`check_get_conditions`（RFC 9110 §13 顺序：`If-Match` → `If-Unmodified-Since` →
  `If-None-Match` → `If-Modified-Since`）接入 **`GetObject` / `HeadObject`**：
  | 情况 | 响应 |
  | --- | --- |
  | `If-None-Match` 命中 / `If-Modified-Since` 未更新 | **`NotModified`（HTTP 304 ✗ 无正文 = 不读 blob，缓存路径省 IO）** |
  | `If-Match` 不符 / `If-Unmodified-Since` 已更新 | `PreconditionFailed` 412 |
  | 其余 | 200（Range 等其余语义不变） |
- **真机验收（真 SDK，10 项）**：`If-None-Match` 命中 304 / 他值 200；`If-Match` 命中 200 / 不符 412；
  `If-Modified-Since` 未来 304 / 过去 200；`If-Unmodified-Since` 过去 412 / 未来 200；
  `HEAD` 条件同样 304；`Range` 回归 ✓。
- **回归**：自写探针 **24/24** · 真 SDK **41/41**（+条件读）。
- **仍债**：`ListObjectVersions` / `PutBucketVersioning`（真版本控制）· 逐键 `LastModifiedTime`/`Size` 条件 ·
  region 校验（有意不做）。

## 状态（r29 末 · 批量删的逐键 ETag 条件 = 并发删安全）

- **实装**：`DeleteObjects` 逐键读 `ObjectIdentifier.e_tag` → 与目标当前 ETag 对账，不符**只拒该键**
  （`Errors` 一项 `PreconditionFailed`，不拖累整批）；**缺失键 + 带条件 = 条件不可满足 → 拒**
  （幂等语义仅对**无条件**删适用）；无条件键行为不变（存在即删 / 缺失幂等 `Deleted`）。
- **真机验收（真 SDK，7 项）**：条件匹配者删 + 无条件者删 ✓ · 条件不符者 `PreconditionFailed` ✓ ·
  被拒对象仍在 ✓ · 已删对象消失 ✓ · 正确 ETag 重试成功 ✓ · 缺失键带条件 → 拒 ✓ ·
  缺失键无条件 → 幂等 `Deleted` ✓。
- **回归**：自写探针 **24/24** · 真 SDK **40/40**（+逐键条件）。
- **仍债**：`ListObjectVersions` / `PutBucketVersioning`（真版本控制）· `LastModifiedTime`/`Size` 逐键条件 ·
  region 校验（有意不做）。

## 状态（r28 末 · 目标条件头 = 现代 S3 乐观并发写）

- **实装**：`check_dest_conditions`（目标条目 `If-Match` / `If-None-Match`，缺失 = 视为不存在）→
  不满足 `PreconditionFailed` 412；接入 **`PutObject`**（条件建/条件改）· **`DeleteObject`**（条件删）·
  **`CopyObject`** 的**目标**条件（与 `copy-source-if-*` 源条件相互独立）；`If-Match: *` = 必须存在，
  `If-None-Match: *` = 必须不存在。
- **真机验收（真 SDK，12 项）**：`If-None-Match:*` 首写 ok / 已存在 412 / **412 后内容未被改写**；
  `If-Match` 正确 ok / 过期 412 / `*` 存在 ok；`DELETE If-Match` 过期 412（对象仍在）/ 正确 ok；
  `CopyObject` 目标 `If-None-Match:*` 新键 ok / 已存在 412。
- **回归**：自写探针 **24/24** · 真 SDK **39/39**（+条件写/删）。
- **仍债**：`ListObjectVersions` / `PutBucketVersioning`（真版本控制）· `DeleteObjects` 逐键条件头 ·
  region 校验（有意不做）。

## 状态（r26 末 · 多租户隔离 = 每凭证一个命名空间视图）

- **实装**：
  | 项 | 内容 |
  | --- | --- |
  | 配置 | `VFILES_S3_CREDENTIALS` 条目形扩为 **`access:secret[:命名空间slug][:ro]`**（第三段非 `ro/rw` 词即 slug；第四段为模式） |
  | 解析 | 新 `NamespaceRepo::find_by_slug`（默认 `None` 桩零破；SQLite = `SELECT id, owner_user_id FROM namespaces WHERE slug=? ORDER BY created_at LIMIT 1`）✗ 取**属主**作 S3 owner |
  | 分发 | 新增 **`S3Router`**（`default_service` + `by_key: access → VfilesS3`）✗ `impl S3 for S3Router` **逐方法委托**（19 个已实现操作 ✗ 现有服务逻辑**零改动** = 由构造保证正确性） |
  | 兜底 | slug 不存在/解析失败 → **回落默认命名空间** + warn（不静默失败）；只读标记随绑定服务各自生效 |
- **真机验收（真 SDK，真基建）**：DB 里 seed 第二个命名空间 `tenant2` → 8 项：
  | 场景 | 结果 |
  | --- | --- |
  | 默认凭证写入 | 只见自己 1 项 ✓ |
  | `tenant2` 凭证写入 | 只见自己 1 项 ✓ |
  | `tenant2` 读默认命名空间对象 | `NoSuchKey` ✓ |
  | 默认凭证读 tenant2 对象 | `NoSuchKey` ✓ |
  | `tenant2` 只读键（`:tenant2:ro`） | 可读、写被拒 `AccessDenied` ✓ |
  | 拒后 tenant2 仍只见自己 1 项 | ✓ |
- **回归**：自写探针 **24/24** · 真 SDK **38/38**（无绑定参数）/ **40/40**（带绑定参数 ✗ 含隔离 + 只读组合）。
- **仍债**：`ListObjectVersions`/`PutBucketVersioning`（真版本控制）· `S3Router` 每方法委托样板（19 段）·
  region 校验（有意不做 = 兼容优先）。

## 状态（r25 末 · 桶级探测补齐 = 客户端连接检查不再报错）

- **实装**（真客户端常在连接/建桶流程里先探测这三式）：
  | 操作 | 行为 |
  | --- | --- |
  | `HeadBucket` | 存在 → 200 空；不存在 → `NoSuchBucket`（HTTP 404） |
  | `GetBucketLocation` | `LocationConstraint = us-east-1`（常量 `S3_REGION`；**签名区域不校验** = 客户端可用任意 region 配置签名） |
  | `GetBucketVersioning` | 无 `Status` = unversioned（与 AWS 未启用版本控制同形） |
- **真机验收（真 SDK）**：`head_bucket` 200 ✓ · 不存在桶 404 ✓ · `get_bucket_location` = `us-east-1` ✓ ·
  `get_bucket_versioning` 无 `Status` ✓ · `list_buckets` / put / get 回归 ✓。
- **回归**：自写探针 **24/24** · 真 SDK **38/38**（+桶级探测）。
- **仍债**：凭证→命名空间绑定（需 domain 按名查命名空间 + 逐请求解析）· `ListObjectVersions` /
  `PutBucketVersioning`（真版本控制）· region 校验（有意不校验 = 兼容优先）。

## 状态（r24 末 · `x-amz-copy-source-if-*` 四头 = 并发拷贝安全）

- **实装**：`check_copy_conditions` 统一门控 4 个头（RFC 9110 §13 + S3 语义）：
  | 头 | 语义 | 不满足 |
  | --- | --- | --- |
  | `copy-source-if-match` | 源 ETag 命中才拷（`*` 恒真） | `PreconditionFailed` 412 |
  | `copy-source-if-none-match` | 源 ETag **不**命中才拷 | 412 |
  | `copy-source-if-modified-since` | 源修改时间**晚于**该时刻才拷 | 412 |
  | `copy-source-if-unmodified-since` | 源修改时间**不晚于**该时刻才拷 | 412 |
  - 源身份（`source_identity`：ETag = `current_version_id` hex + `created_at`）同时修掉「源不存在」的错误码
    （现为 `NoSuchKey` 而非链路错）
  - **`CopyObject` 与 `UploadPartCopy` 同门控**（两处都带这 4 个头）
- **真机验收（真 SDK，11 项）**：if-match 正确/错误 → ok/412；if-none-match 命中/他值 → 412/ok；
  if-modified-since 过去/未来 → ok/412；if-unmodified-since 未来/过去 → ok/412；拷贝正文仍正确；
  `upload_part_copy` 的 if-match 正确/错误 → ok/412。
- **回归**：自写探针 **24/24** · 真 SDK **37/37**（+条件复制）。
- **仍债**：凭证→命名空间绑定 · region 校验。

## 状态（r23 末 · 元数据三路齐全 = PUT / Copy / multipart）

- **实装**：
  | 项 | 内容 |
  | --- | --- |
  | 会话元数据 | `UploadStore::set/get_upload_custom_metadata`（存 `metadata.json` 的 `custom_metadata` 键）+ app 包装 |
  | Create | `CreateMultipartUpload` 的 `x-amz-meta-*` 存**会话**（条目此时不存在） |
  | Complete | **先读会话元数据再完成**（完成会清会话目录 ✗ 顺序错 = 元数据丢失，真机抓到）→ 落到 entry 属性（覆盖写语义：无元数据即清空） |
  | Abort | 会话目录销毁 = 元数据随之消失（对象不存在） |
- **真机验收（真 SDK）**：
  | 场景 | 结果 |
  | --- | --- |
  | Create 带元数据 → 分片 → 完成 | GET/HEAD **原样返回** + 正文完整 ✓ |
  | 完成时无元数据（覆盖已有对象） | 元数据**清空**（S3 语义：由 Create 决定） ✓ |
  | Abort | 不产生对象 ✓ |
  | 之后普通 PUT（无元数据） | 清空 ✓ |
- **回归**：自写探针 **24/24** · 真 SDK **36/36**（+multipart 元数据）。
- **仍债**：凭证→命名空间绑定 · region 校验 · `copy_source_if_*` 条件头。

## 状态（r22 末 · `x-amz-meta-*` 往返 = 客户端自定义元数据可用）

- **实装**（复用 0006 `entry_properties` 表 = **零迁移**）：
  | 项 | 内容 |
  | --- | --- |
  | 存储 | 前缀 `s3-meta:` 入 entry 属性（s3s `input.metadata` = `x-amz-meta-*` 已解析的 HashMap） |
  | 读取 | `GetObject` / `HeadObject` 经 `list_entry_properties` 回填 `output.metadata` → 响应头 |
  | 覆盖写 | `PutObject` 先清旧 `s3-meta:*` 再写新 = 「无元数据即清空」S3 语义 |
  | 复制 | `CopyObject` 默认（COPY）**抄源条目元数据**；`MetadataDirective=REPLACE` 用请求值（无则清空）；源条目不受影响 |
  | 级联 | 删 entry → 属性随 FK **ON DELETE CASCADE** 自动清 |
- **真机验收（真 SDK）**：
  | 场景 | 结果 |
  | --- | --- |
  | PUT 带 3 项元数据 | GET/HEAD 原样返回 ✓ |
  | 无元数据覆盖写 | 元数据**清空** ✓ |
  | `CopyObject` 默认 | 元数据**继承源** ✓ |
  | `REPLACE` + 新元数据 / `REPLACE` 无元数据 | 替换 / **清空** ✓ |
  | 源条目元数据 | 未被改动 ✓ |
- **回归**：自写探针 **24/24** · 真 SDK **35/35**（+4 元数据检查）。
- **仍债**：多段上传的 `x-amz-meta-*`（`CreateMultipartUpload` 传入 ✗ 会话无此字段）· 凭证→命名空间
  绑定 · region 校验 · `copy_source_if_*` 条件头。

## 状态（r21 末 · 按凭证的读写权限 = 多客户端安全基线）

- **实装**：`VFILES_S3_CREDENTIALS` 条目扩展为 `access:secret[:ro]`（`rw`/缺省 = 读写 ✗ 未知模式按读写
  并 warn）：
  - `EnvAuth` 仍只做密钥校验；**权限面**由 `VfilesS3.readonly_keys` 承载
  - s3s 把 SigV4 解析出的 `Credentials{access_key}` 放在 `S3Request.credentials` → 服务端按请求判别
  - `require_write(creds)` 命中只读键 → `AccessDenied`，覆盖**全部 9 个变更类操作**：
    `PutObject` / `DeleteObject` / `DeleteObjects` / `CopyObject` / `CreateMultipartUpload` /
    `UploadPart` / `UploadPartCopy` / `CompleteMultipartUpload` / `AbortMultipartUpload`
  - 读类（`GetObject` / `HeadObject` / `ListObjects(V2)` / `ListParts` / `ListMultipartUploads`）不受限
- **真机验收（3 键同服：单对 rw + `rw-key` + `ro-key:…:ro`）**：
  | 场景 | 结果 |
  | --- | --- |
  | `rw` 键写 + 读 | ✓ |
  | `ro` 键读 + 列 | ✓ |
  | `ro` 键 PUT / DELETE / 批量 DELETE / CopyObject / CreateMPU / UploadPart / UploadPartCopy / Abort | **8/8 `AccessDenied`** ✓ |
  | 拒后数据未被改动 | ✓ |
  | 单对（env `ACCESS_KEY/SECRET_KEY`）仍 rw | ✓ |
- **回归**：自写探针 **24/24** · 真 SDK **31/31**（`boto_probe.py` 第 6 参 = 第二凭证只读标志，ro/rw 两
  种服务端配置各跑一遍均 31/31）。
- **仍债**：凭证→**命名空间**绑定（需 domain 加按名查命名空间）· region 校验 · `copy_source_if_*`。

## 状态（r20 末 · 列表 SQL 逐页 = 不物化整桶）

- **实装**：
  | 项 | 内容 |
  | --- | --- |
  | `EntryRepo::files_with_meta_page` | 新仓储方法（默认 = `files_with_meta` 过滤切片 ✗ 桩零破）；**SQLite 覆写 = 一条 SQL** `WHERE kind='file' AND path > ? ORDER BY path LIMIT ?`（BINARY 排序 = Rust `str` Ord 一致） |
  | `ListCollector` | **流式收集器**（按 key 升序喂入 ✗ 与存储解耦）：prefix 过滤 → delimiter 连续同组去重 → `after` 独占续页 → 收 `max+1` 判截断；`prefix_upper_bound` 越过即停扫 |
  | `list_page` | 1000/批拉取 ✗ 收满即停（`max_keys=1` 只取 1~2 行）→ **内存 = O(max_keys) 而非 O(桶)** |
  | 续页游标 | 直接从 `after` 之后扫原始 key（滚出条目单调不减 → 不丢条目不重复；代码附论证） |
- **等价性保证**：`build_entries`+`paginate` 降为 `#[cfg(test)]` **参考实现** ✗ 新单测对 6 组 prefix × 2 组
  delimiter × `max=3` **逐页走全**，断言与参考实现 key 序列**完全一致且无重复**。
- **真机验收（500 对象 ×10 目录）**：
  | 场景 | 结果 |
  | --- | --- |
  | `PageSize=100` 分页 | 5 页 / 500 key **无重无漏** + 字典序升序 ✓ |
  | `delimiter=/` | 10 个 CommonPrefixes ✓ |
  | V1 `MaxKeys=120` | `IsTruncated` + `NextMarker` + marker 续页 ✓ |
  | `StartAfter` | 严格大于所给 key ✓ |
  | `MaxKeys=1` 单键续页 | **500 次走全 500 key（1.9s）** ✓ |
- **回归**：自写探针 **24/24** · 真 SDK **30/30**（+V1 marker / StartAfter / 单键续页）。
- **仍债**：访问键绑用户 · region 校验 · `copy_source_if_*` 条件头。

## 状态（r19 末 · 分片复制落地 = 大对象跨键拷贝路径）

- **实装**：`UploadPartCopy`（`PUT ?partNumber&uploadId` + `x-amz-copy-source`）：
  - `copy_source` = `<bucket>/<key>`（s3s `CopySource::Bucket`）；同桶校验
  - `copy_source_range` = `bytes=start-end`（闭区间 ✗ 亦支持 `bytes=start-` 开尾与 `bytes=-N` 后缀；
    尾越界夹取、起点越界/逆序 → `InvalidRange`）
  - 读源字节 → `upload_part` 落分片 → 返回 `CopyPartResult{ETag=MD5(分片), LastModified}`
- **实证（真 SDK）**：
  | 场景 | 结果 |
  | --- | --- |
  | 1MiB 源分两段复制（`0-524287` / `524288-`）→ 完成 → `GetObject` | **1,048,576 B 逐字节同** ✓ |
  | `ListParts` ETag 对比复制返回 | 逐一相等 ✓ |
  | 无 range 整对象复制 | 内容同 ✓ |
  | 起点越界 | `InvalidRange`/`InvalidArgument` ✓ |
- **回归**：自写探针 **24/24** · 真 SDK **27/27**（+UploadPartCopy）。
- **拷贝面**：`CopyObject`（小对象/单请求）+ `UploadPartCopy`（大对象/分片）**双式齐全**。
- **仍债**：SQL 级 prefix/limit 分页 · 访问键绑用户 · region 校验 · `copy_source_if_*` 条件头 ·
  `MetadataDirective` 于 `UploadPartCopy`（本无意义）。

## 状态（r16 末 · multipart API 收口 = 六式齐全）

- **实装**：
  | 项 | 内容 |
  | --- | --- |
  | `UploadStore::list_upload_sessions` | 新仓储方法（扫上传目录 ✗ 损坏/半成品目录跳过）+ app 按命名空间过滤 |
  | `ListMultipartUploads` | 只列 `Receiving` 状态；支持 `prefix` / `delimiter` 折叠 / `max-uploads` / `key-marker` + `upload-id-marker` 续页（同 key 多会话按 upload id 定序）|
- **实证（真 SDK）**：
  | 场景 | 结果 |
  | --- | --- |
  | 5 个进行中上传 | 全部列出 + `Initiated` 存在 ✓ |
  | `prefix=lmu/` | 4 条 ✓ |
  | `prefix=lmu/ delimiter=/` | `lmu/sub/` 折叠 + 顶层两条 ✓ |
  | `max-uploads=2` + marker 续页 | 截断 + 续页无重 ✓ |
  | abort 一个后 | 该会话消失 ✓ |
- **回归**：自写探针 **24/24** · 真 SDK **26/26**（+2 ListMultipartUploads）。
- **multipart 面**：`Create` / `UploadPart` / `Complete`（含校验）/ `Abort` / `ListParts`（含 ETag）/
  `ListMultipartUploads` **六式齐全**。
- **仍债**：SQL 级 prefix/limit · 访问键绑用户 · region 校验 · `UploadPartCopy`。

## 状态（r15 末 · part ETag + 完成校验 = multipart 不静默出错）

- **实装**：
  | 项 | 内容 |
  | --- | --- |
  | `UploadStore::read_upload_part` | 新仓储方法（读单个分片 ✗ SQLite 覆写 = 直读 `part_<i>` 文件）+ `UploadService::read_upload_part` 包装 |
  | `ListParts` ETag | 逐分片读回 → `ETag = MD5(分片)`（此前 `ETag` 恒缺 = 客户端拿不到 part 校验值） |
  | `CompleteMultipartUpload` 校验 | ① 客户端回显的 part ETag 与存储分片 MD5 **逐一对账**，不符 → `InvalidPart`；② **列出集合必须等于已存集合**（否则拼接会按全部已存分片进行 = 内容静默错位）→ `InvalidPart` |
- **实证（真 SDK）**：
  | 场景 | 结果 |
  | --- | --- |
  | 4×256KiB 分片 → `list_parts` ETag 对比 `upload_part` 回显 | **逐一相等** ✓ |
  | 完成时篡改一个 ETag | `InvalidPart` ✓ |
  | 完成时只列子集（1,3） | `InvalidPart` ✓ |
  | 正确完成 → `GetObject` | 1,048,576 B 逐字节同 ✓ |
- **回归**：自写探针 **24/24** · 真 SDK **24/24**（新增 2 项 multipart 完整性检查）。
- **仍债**：`ListMultipartUploads`（需列活跃会话）· SQL 级 prefix/limit · 访问键绑用户 · region 校验 ·
  `UploadPartCopy`。

## 状态（r13 末 · 大桶列表可扩展 = 250 对象 11.7ms / 分页无重无漏）

- **实装**：
  | 项 | 内容 |
  | --- | --- |
  | `EntryRepo::files_with_meta` | 新仓储方法（默认 = `find_all` 过滤 ✗ 桩零破）；**SQLite 覆写 = 一条 SQL**（`entries WHERE kind='file'` + 最新版本 size/content_type 标量子查询 ✗ `children_with_meta` 同款列） |
  | `collect_objects` | 由**逐目录递归**（目录数条查询 = N+1）改为**一次取全 + 内存排序** |
- **实证**：
  | 场景 | 结果 |
  | --- | --- |
  | 250 对象 ×10 目录 全列 | **11.7ms**（一条 SQL） |
  | `max-keys=100` 分页 | 3 页 / 250 key **无重无漏** ✓ |
  | `delimiter=/` | 10 个 CommonPrefixes ✓ |
  | 字典序 | `f0,f1,f10…` = 路径升序 ✓ |
  | 回归 | 自写探针 **24/24** · 真 SDK **22/22**（新增 120 对象分级分页 + 4 前缀 delimiter） |
- **含义**：列表成本从「目录数」降到「一次查询」✗ 十万级对象的桶不再随目录数劣化；
  真正的 SQL 级 prefix/续页（避免全量入内存）仍是后续债。
- **仍债**：SQL 级 prefix/limit 分页（现全量入内存）· `ListMultipartUploads` · 访问键绑用户 ·
  region 校验 · part 哈希持久化。

## 状态（r10 末 · 多凭证 / 服务端复制 / Content-Type 保真 ✗ 探针 24/24 + 真 SDK 21/21）

- **新增/修复**：
  | 项 | 内容 |
  | --- | --- |
  | **多凭证** | `VFILES_S3_CREDENTIALS=access:secret,...`（与单对 `ACCESS_KEY/SECRET_KEY` 并存）✗ `EnvAuth` 改为 `access → secret` 表；未知 key = `InvalidAccessKeyId` 403 |
  | **`CopyObject`** | 同桶 `PUT` + `x-amz-copy-source`（s3s 解析为 `CopySource::Bucket`）：读源字节 → 上传链落目标；`MetadataDirective=REPLACE` 用请求 ContentType，否则沿用源 MIME；返回 `CopyObjectResult{ETag, LastModified}` |
  | **Content-Type 贯通（真 bug）** | 上传链此前**丢弃**客户端 `Content-Type`（`init_upload` `let _ = mime_type` + 提交时按扩展名猜 ✗ 实测 `application/x-thing` 变 `application/octet-stream`）→ 现 `UploadSession.mime_type` 全链贯通（domain 字段 + store 元数据 + 提交优先取声明值）✗ 同时惠及 WebDAV/HTTP 上传 |
- **实证（双通道）**：
  | 通道 | 结果 |
  | --- | --- |
  | `scripts/sigv4_probe.py` | **24/24**（新增 23 CopyObject 字节 + ContentType） |
  | `scripts/boto_probe.py`（可选第 2 凭证参数） | **21/21**（新增 CopyObject + 第二凭证可用 + 未知凭证被拒） |
- **仍债**：`ListMultipartUploads` · per-user 授权粒度（现访问键不绑定用户）· region 校验 · 大桶 SQL 分页 · part 哈希持久化。

## 状态（r6 末 · 大文件流式上传 + 批量删 = 客户端日常动线补齐）

- **新增/改动**（`crates/vfiles-s3/src/lib.rs`）：
  | 操作 | 变化 |
  | --- | --- |
  | `PutObject` | Content-Length 已知 → **流式直连**（`StreamReader` → `complete_upload_from_stream` ✗ 不再全量入内存）；未知长度 → 聚合回退；返回 **ETag + Size** |
  | `DeleteObjects` | 批量删（逐键点查分区 → 存在者**一次批量删**=单快照；缺失键按 S3 幂等记 `Deleted`；`Quiet` 抑制条目；非法键进 `Error`） |
- **踩坑记录**：`delete_entries` 遇**任一**路径缺失即整体 `NotFound`（不删任何键）✗ 直接批量传含缺失键的列表 = "报成功但没删"（探针 6.4 抓到）→ 修正为**先分区再批量**。
- **实证（入仓两通道）**：
  | 通道 | 结果 |
  | --- | --- |
  | `scripts/sigv4_probe.py`（自写 SigV4） | **23/23**（r2 九 + r3 八 + r4 四 + r6 二：`DeleteObjects` 含缺失键 / 删除后列表清空） |
  | `scripts/boto_probe.py`（**真 AWS SDK**） | **19/19**（+ **8MiB 流式 PUT 往返字节同** + `DeleteObjects` 全报删/Quiet） |
- **仍债**：`ListMultipartUploads` · per-user 凭证 · region 校验 · 大桶 SQL 分页 · part 哈希持久化（ListParts 无 ETag）。

## 状态（r4 末 · multipart 上传打通 = 大文件客户端路径可用）

- **新增能力**（`crates/vfiles-s3/src/lib.rs` + `crates/vfiles-app/src/services.rs`）：
  | 操作 | 映射 |
  | --- | --- |
  | `CreateMultipartUpload` | `UploadService::init_multipart_upload`（**未知总大小**会话：`declared_size=0`/`total_chunks=0`）→ 返回 uploadId = 上传会话 id |
  | `UploadPart` | `upload_part(upload_id, partNumber-1, data)`（part 号 1..=10000 ✗ 内部零基）× 返回 **part MD5 ETag** |
  | `CompleteMultipartUpload` | 校验列出 part 均已上传 → `assemble_upload_stream`（按 0..n-1 拼接）→ `complete_multipart_upload`（**跳过量校验** ✗ 总大小未知） |
  | `AbortMultipartUpload` | `cancel_upload`（幂等） |
  | `ListParts` | `list_upload_parts`（partNumber / size / last-modified） |
- **app 层改动**：`commit_upload_stream` 增 `enforce_size: bool`（既有调用传 `true`）＋ 三个新方法
  （`init_multipart_upload` / `complete_multipart_upload` / `list_upload_parts`）。复用既有
  `upload_sessions`+分片文件存储，无新表。
- **实证（入仓两通道）**：
  | 通道 | 结果 |
  | --- | --- |
  | `scripts/sigv4_probe.py`（自写 SigV4） | **21/21**（r2 九式 + r3 八式 + r4 四式：create/upload/list_parts/complete+字节校验/abort） |
  | `scripts/boto_probe.py`（**真 AWS SDK**） | **14/14**（9 列表/Range + 5 multipart：4×1MB 分片 → 拼接字节逐字同 → abort） |
- **约束/债（记档）**：part 必须**零基连续**（跳号 = assemble UploadConflict；客户端常规 1..n 无碍）·
  完成时**不校验 part ETag**、`ListParts` 不返回 ETag（part 哈希未持久化）· `ListMultipartUploads`
  未实现 · 最终 ETag = 版本 id（非 AWS 复合形，但与 GET/HEAD 自洽）· put 流式直连 / per-user 凭证 /
  region 校验 / 大桶 SQL 分页仍在债。

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
