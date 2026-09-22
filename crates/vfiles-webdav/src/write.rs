//! WebDAV 写门面（r108' ✓ 与 `EntryRepo` 同式 dyn-trait ✓ bin 侧转发
//! `DefaultWorkspaceService::create_directory/move_entries/delete_entries`）。
//!
//! `user_id` 贯通审计链（后端 MutationResult 记录 ✓ 语义 = Web/FTP 一致）。
//! PUT = `init_upload` 链（分片式）→ r109'（与覆盖上传提案语义联动 ✓）；
//! COPY = 无后端 copy API → **501 记档**（rclone 用 GET+PUT 不依赖 ✓）。

#![allow(dead_code)]

use async_trait::async_trait;
use vfiles_domain::types::{NamespaceId, NormalizedPath, UserId};
use vfiles_domain::DomainResult;

#[async_trait]
pub trait WebdavWriteOps: Send + Sync {
    /// PUT（r110'b ✓ 商业级写面终件）：流式直传（`init_upload` + `complete_upload_from_stream` ✓
    /// 免分片循环（小文件直传 ✓ 大文件 = HTTP 上传管线同源 ✓）。
    async fn put_file(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
        data: Vec<u8>,
        user_id: &UserId,
    ) -> DomainResult<()>;
    async fn mkcol(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
        user_id: &UserId,
    ) -> DomainResult<()>;
    async fn move_entry(
        &self,
        namespace_id: &NamespaceId,
        from: &NormalizedPath,
        to: &NormalizedPath,
        user_id: &UserId,
    ) -> DomainResult<()>;
    async fn delete_entry(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
        user_id: &UserId,
    ) -> DomainResult<()>;
}
