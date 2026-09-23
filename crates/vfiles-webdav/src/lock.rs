//! WebDAV 排他写锁（RFC 4918 §6 子集 ✓ r109a 商业级核心件）。
//!
//! - **内存锁表**（进程内 ✓ 多副本部署 = 后续记档；重启丢锁 = 客户端可续传语义 ✓）；
//! - **exclusive write lock**（shared lock = 409/记档 ✗ 客户端实用仅 exclusive ✓）；
//! - **depth = 0 限定**（infinite = 412 记档 ✓ 实用面覆盖）；
//! - **timeout** = 忽略取默认（∞ ✓ RFC 允许服务端限定 ✓ 记档）。
//!
//! 写操作校验（MKCOL/DELETE/MOVE/PUT…）：被锁路径无 `If` token = **423 Locked** ✓。

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// 单条锁（exclusive write ✓ depth 0 ✓）。
#[derive(Debug, Clone)]
pub struct LockEntry {
    pub token: String,
    pub owner: String,
    pub path: String,
    /// r15 有限超时（None = Infinite ✗ 惰性过期 = 查询/加锁时判定 ✓ 零后台任务）。
    pub expires_at: Option<std::time::Instant>,
}

/// 内存锁表（key = 归一化路径 ✓）。
#[derive(Default)]
pub struct LockTable {
    locks: Mutex<HashMap<String, LockEntry>>,
    seq: AtomicU64,
}

impl LockTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// 生成锁令牌（`opaquelocktoken:` + 零依赖 hex（纳秒+计数）✓ RFC 形）。
    fn mint(&self) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        format!("opaquelocktoken:{nanos:x}{seq:04x}")
    }

    /// 加锁（已锁 = 冲突 → None（调用方 423/或回已有锁 ✓ 简式 = None→423 ✓）。
    pub fn lock(
        &self,
        path: &str,
        owner: &str,
        ttl: Option<std::time::Duration>,
    ) -> Option<LockEntry> {
        let mut map = self.locks.lock().expect("lock table poisoned");
        // r15 惰性过期：现存条目已过期 → 视同可覆盖（新锁直接接管 ✓ 无需后台任务）
        if let Some(cur) = map.get(path) {
            if !Self::expired(cur) {
                return None;
            }
        }
        let entry = LockEntry {
            token: self.mint(),
            owner: owner.to_string(),
            path: path.to_string(),
            expires_at: ttl.map(|d| std::time::Instant::now() + d),
        };
        map.insert(path.to_string(), entry.clone());
        Some(entry)
    }

    /// 过期判定（r15 纯逻辑 ✓ 无期限 = 永不过期）。
    fn expired(entry: &LockEntry) -> bool {
        entry
            .expires_at
            .map(|t| std::time::Instant::now() >= t)
            .unwrap_or(false)
    }

    /// 解锁（token 匹配才解 ✓ 不匹配 = None（调用方 409 Conflict ✓）。
    pub fn unlock(&self, path: &str, token: &str) -> Option<LockEntry> {
        let mut map = self.locks.lock().expect("lock table poisoned");
        if let Some(entry) = map.get(path) {
            if Self::expired(entry) {
                map.remove(path); // r15 过期锁 unlock = 清 + None（调用方旧映射照旧 ✓）
                return None;
            }
            if entry.token == token {
                return map.remove(path);
            }
        }
        None
    }

    /// 刷新匹配 token 的锁时限；已过期、路径错误或 token 错误均不改变锁表。
    pub fn refresh(
        &self,
        path: &str,
        token: &str,
        ttl: Option<std::time::Duration>,
    ) -> Option<LockEntry> {
        let mut map = self.locks.lock().expect("lock table poisoned");
        let expired = map.get(path).is_some_and(Self::expired);
        if expired {
            map.remove(path);
            return None;
        }
        let entry = map.get_mut(path)?;
        if entry.token != token {
            return None;
        }
        entry.expires_at = ttl.map(|duration| std::time::Instant::now() + duration);
        Some(entry.clone())
    }

    /// 写操作锁校验（被锁路径 → Some(entry)（调用方 423 ✓ 无锁 → None ✓）。
    pub fn blocked(&self, path: &str) -> Option<LockEntry> {
        let mut map = self.locks.lock().expect("lock table poisoned");
        // r15 惰性过期（写前置/锁查主路径 ✗ 到期即释放 = 下一访问生效 ✓）
        if let Some(entry) = map.get(path) {
            if Self::expired(entry) {
                map.remove(path);
                return None;
            }
            return Some(entry.clone());
        }
        None
    }

    /// Timeout 头解析（r15 ✓ 纯函数单测）：`Second-N` → Some(N 秒) ✗ `Infinite`/无效/
    /// 多值（逗号列表取首个可解析的 Second-N）→ None（= 永久语义）。
    pub fn parse_timeout_header(value: &str) -> Option<std::time::Duration> {
        for part in value.split(',') {
            let t = part.trim();
            if let Some(rest) = t.strip_prefix("Second-") {
                if let Ok(n) = rest.trim().parse::<u64>() {
                    return Some(std::time::Duration::from_secs(n));
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locks_exclusively_and_mints_opaque_tokens() {
        let table = LockTable::new();
        let a = table.lock("a.txt", "alice", None).expect("first lock");
        assert!(a.token.starts_with("opaquelocktoken:"));
        assert!(table.lock("a.txt", "bob", None).is_none(), "exclusive 冲突 ✓");
    }

    #[test]
    fn unlocks_only_with_matching_token() {
        let table = LockTable::new();
        let entry = table.lock("b.txt", "alice", None).unwrap();
        assert!(table.unlock("b.txt", "wrong").is_none(), "token 不匹配 ✓");
        assert!(table.unlock("b.txt", &entry.token).is_some());
        assert!(table.blocked("b.txt").is_none());
    }

    #[test]
    fn reports_blocked_paths_for_writers() {
        let table = LockTable::new();
        assert!(table.blocked("c.txt").is_none());
        table.lock("c.txt", "alice", None).unwrap();
        assert_eq!(table.blocked("c.txt").map(|e| e.owner), Some("alice".into()));
    }
}

#[cfg(test)]
mod r15_timeout_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn parses_second_n_and_infinite_forms() {
        // r15 Timeout 头守护：Second-N 解析 / Infinite·无效·多值→None（永久语义）
        assert_eq!(
            LockTable::parse_timeout_header("Second-3600"),
            Some(Duration::from_secs(3600))
        );
        assert_eq!(
            LockTable::parse_timeout_header("Second-1, Infinite"),
            Some(Duration::from_secs(1))
        );
        assert_eq!(LockTable::parse_timeout_header("Infinite"), None);
        assert_eq!(LockTable::parse_timeout_header("garbage"), None);
    }

    #[test]
    fn lazy_expiry_frees_locks() {
        // r15 惰性过期守护：过期 → blocked None + lock 可接管 / 未过期 → 仍在
        let table = LockTable::new();
        assert!(table.lock("x", "alice", Some(Duration::from_millis(30))).is_some());
        assert!(table.blocked("x").is_some(), "未过期仍在");
        std::thread::sleep(Duration::from_millis(60));
        assert!(table.blocked("x").is_none(), "过期释放 ✓");
        assert!(table.lock("x", "bob", None).is_some(), "过期接管 ✓");
        // 永久锁不随时间失效（结构上 expires_at = None）
        assert!(table.lock("y", "alice", None).is_some());
        std::thread::sleep(Duration::from_millis(30));
        assert!(table.blocked("y").is_some(), "Infinite 永续 ✓");
    }
}
