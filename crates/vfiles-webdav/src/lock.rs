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
    pub fn lock(&self, path: &str, owner: &str) -> Option<LockEntry> {
        let mut map = self.locks.lock().expect("lock table poisoned");
        if map.contains_key(path) {
            return None;
        }
        let entry = LockEntry {
            token: self.mint(),
            owner: owner.to_string(),
            path: path.to_string(),
        };
        map.insert(path.to_string(), entry.clone());
        Some(entry)
    }

    /// 解锁（token 匹配才解 ✓ 不匹配 = None（调用方 409 Conflict ✓）。
    pub fn unlock(&self, path: &str, token: &str) -> Option<LockEntry> {
        let mut map = self.locks.lock().expect("lock table poisoned");
        match map.get(path) {
            Some(entry) if entry.token == token => map.remove(path),
            _ => None,
        }
    }

    /// 写操作锁校验（被锁路径 → Some(entry)（调用方 423 ✓ 无锁 → None ✓）。
    pub fn blocked(&self, path: &str) -> Option<LockEntry> {
        let map = self.locks.lock().expect("lock table poisoned");
        map.get(path).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locks_exclusively_and_mints_opaque_tokens() {
        let table = LockTable::new();
        let a = table.lock("a.txt", "alice").expect("first lock");
        assert!(a.token.starts_with("opaquelocktoken:"));
        assert!(table.lock("a.txt", "bob").is_none(), "exclusive 冲突 ✓");
    }

    #[test]
    fn unlocks_only_with_matching_token() {
        let table = LockTable::new();
        let entry = table.lock("b.txt", "alice").unwrap();
        assert!(table.unlock("b.txt", "wrong").is_none(), "token 不匹配 ✓");
        assert!(table.unlock("b.txt", &entry.token).is_some());
        assert!(table.blocked("b.txt").is_none());
    }

    #[test]
    fn reports_blocked_paths_for_writers() {
        let table = LockTable::new();
        assert!(table.blocked("c.txt").is_none());
        table.lock("c.txt", "alice").unwrap();
        assert_eq!(table.blocked("c.txt").map(|e| e.owner), Some("alice".into()));
    }
}
