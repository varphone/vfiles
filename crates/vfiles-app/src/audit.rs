//! 审计日志服务：只负责追加与查询。
//!
//! 写入刻意做成「尽力而为」：审计失败不能影响主流程（登录、上传等），
//! 因此 [`AuditService::record`] 只记录警告并返回，不向上传播错误。
//! 数据库层的触发器保证日志只可追加、不可修改或删除。

use tracing::warn;
use vfiles_domain::{
    AuditLogPage, AuditLogQuery, AuditLogRepo, AuditLogSummary, DomainResult, NewAuditLog,
};

#[derive(Clone)]
pub struct AuditService<R> {
    repo: R,
}

impl<R> AuditService<R>
where
    R: AuditLogRepo + Send + Sync,
{
    pub fn new(repo: R) -> Self {
        Self { repo }
    }

    /// 追加一条审计日志；失败只记录警告，不影响调用方。
    pub async fn record(&self, entry: NewAuditLog) {
        if let Err(err) = self.repo.append(&entry).await {
            warn!(
                action = %entry.action,
                error = %err,
                "Failed to append audit log"
            );
        }
    }

    /// 按条件分页查询。
    pub async fn list(&self, query: &AuditLogQuery) -> DomainResult<AuditLogPage> {
        self.repo.list(query).await
    }

    /// 已出现过的动作（用于筛选下拉）。
    pub async fn actions(&self) -> DomainResult<Vec<String>> {
        self.repo.distinct_actions().await
    }

    /// 当前筛选条件下的聚合概览。
    pub async fn summarize(
        &self,
        query: &AuditLogQuery,
        top: u32,
    ) -> DomainResult<AuditLogSummary> {
        self.repo.summarize(query, top).await
    }
}
