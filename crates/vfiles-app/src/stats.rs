//! 导入协议（FTP 等）的运行计数。
//!
//! 与缩略图统计一致：进程级原子计数 + 可序列化快照，健康检查接口直接读取。

use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

/// 进程内累计的导入协议计数。
#[derive(Debug, Default)]
pub struct IngestStats {
    active_sessions: AtomicU64,
    total_sessions: AtomicU64,
    logins_ok: AtomicU64,
    logins_failed: AtomicU64,
    files_uploaded: AtomicU64,
    bytes_uploaded: AtomicU64,
    files_downloaded: AtomicU64,
    bytes_downloaded: AtomicU64,
    snapshots_flushed: AtomicU64,
    errors: AtomicU64,
}

/// 健康检查使用的快照。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct IngestStatsSnapshot {
    pub active_sessions: u64,
    pub total_sessions: u64,
    pub logins_ok: u64,
    pub logins_failed: u64,
    pub files_uploaded: u64,
    pub bytes_uploaded: u64,
    pub files_downloaded: u64,
    pub bytes_downloaded: u64,
    pub snapshots_flushed: u64,
    pub errors: u64,
}

impl IngestStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn session_started(&self) {
        self.active_sessions.fetch_add(1, Ordering::Relaxed);
        self.total_sessions.fetch_add(1, Ordering::Relaxed);
    }

    pub fn session_finished(&self) {
        // 异常路径可能重复调用，这里做饱和处理避免下溢
        let _ = self
            .active_sessions
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some(value.saturating_sub(1))
            });
    }

    pub fn record_login_success(&self) {
        self.logins_ok.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_login_failure(&self) {
        self.logins_failed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_upload(&self, bytes: u64) {
        self.files_uploaded.fetch_add(1, Ordering::Relaxed);
        self.bytes_uploaded.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn record_download(&self, bytes: u64) {
        self.files_downloaded.fetch_add(1, Ordering::Relaxed);
        self.bytes_downloaded.fetch_add(bytes, Ordering::Relaxed);
    }

    /// 记录一次快照提交；`count` 为本次提交数量（通常为 1）。
    pub fn record_snapshot_flush(&self, count: u64) {
        if count > 0 {
            self.snapshots_flushed.fetch_add(count, Ordering::Relaxed);
        }
    }

    pub fn record_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> IngestStatsSnapshot {
        IngestStatsSnapshot {
            active_sessions: self.active_sessions.load(Ordering::Relaxed),
            total_sessions: self.total_sessions.load(Ordering::Relaxed),
            logins_ok: self.logins_ok.load(Ordering::Relaxed),
            logins_failed: self.logins_failed.load(Ordering::Relaxed),
            files_uploaded: self.files_uploaded.load(Ordering::Relaxed),
            bytes_uploaded: self.bytes_uploaded.load(Ordering::Relaxed),
            files_downloaded: self.files_downloaded.load(Ordering::Relaxed),
            bytes_downloaded: self.bytes_downloaded.load(Ordering::Relaxed),
            snapshots_flushed: self.snapshots_flushed.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_sessions_transfers_and_errors() {
        let stats = IngestStats::new();
        stats.session_started();
        stats.session_started();
        stats.session_finished();
        stats.record_login_success();
        stats.record_login_failure();
        stats.record_upload(1024);
        stats.record_download(2048);
        stats.record_snapshot_flush(2);
        stats.record_error();

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.active_sessions, 1);
        assert_eq!(snapshot.total_sessions, 2);
        assert_eq!(snapshot.logins_ok, 1);
        assert_eq!(snapshot.logins_failed, 1);
        assert_eq!(snapshot.files_uploaded, 1);
        assert_eq!(snapshot.bytes_uploaded, 1024);
        assert_eq!(snapshot.files_downloaded, 1);
        assert_eq!(snapshot.bytes_downloaded, 2048);
        assert_eq!(snapshot.snapshots_flushed, 2);
        assert_eq!(snapshot.errors, 1);
    }

    #[test]
    fn active_sessions_never_underflow() {
        let stats = IngestStats::new();
        stats.session_finished();
        assert_eq!(stats.snapshot().active_sessions, 0);
    }

    #[test]
    fn snapshot_exposes_all_counters() {
        // 序列化由 health 路由完成；这里只校验字段齐全，避免引入 dev 依赖
        let stats = IngestStats::new();
        stats.record_upload(5);
        let snapshot = stats.snapshot();
        assert_eq!(snapshot.files_uploaded, 1);
        assert_eq!(snapshot.bytes_uploaded, 5);
        assert_eq!(snapshot.snapshots_flushed, 0);
    }
}
