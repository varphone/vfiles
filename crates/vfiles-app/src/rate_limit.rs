//! 登录失败限流（固定时间窗）。
//!
//! HTTP 登录与 FTP 认证共用：按「用户名 + 来源」计数，超过阈值后在一段时间内
//! 直接拒绝，避免离线爆破。计数器是尽力而为的内存状态，进程重启即清空。

use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

const MAX_LOGIN_COUNTERS: usize = 50_000;
const MAX_RATE_LIMIT_WINDOW: Duration = Duration::from_secs(365 * 24 * 60 * 60);
const FALLBACK_RATE_LIMIT_WINDOW: Duration = Duration::from_secs(24 * 60 * 60);

fn deadline_after(now: Instant, window: Duration) -> Instant {
    now.checked_add(window.min(MAX_RATE_LIMIT_WINDOW))
        .or_else(|| now.checked_add(FALLBACK_RATE_LIMIT_WINDOW))
        .unwrap_or(now)
}

/// 限流策略（由各协议层从自身配置转换而来，避免这里依赖配置文件解析）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitPolicy {
    pub enabled: bool,
    pub window_ms: u64,
    pub max_attempts: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoginRateLimitBlock {
    pub retry_after_secs: u64,
}

#[derive(Debug)]
struct LoginAttemptCounter {
    failures: u32,
    reset_at: Instant,
}

#[derive(Debug, Default)]
struct LoginAttemptState {
    counters: HashMap<[u8; 32], LoginAttemptCounter>,
}

#[derive(Debug, Default)]
pub struct LoginAttemptLimiter {
    state: Mutex<LoginAttemptState>,
}

impl LoginAttemptLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, LoginAttemptState> {
        // A poisoned lock must not turn a login rejection into a process panic.
        // The counters are best-effort, so recovering the previous state is safe.
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn key_digest(key: &str) -> [u8; 32] {
        Sha256::digest(key.as_bytes()).into()
    }

    pub fn check(&self, config: &RateLimitPolicy, key: &str) -> Option<LoginRateLimitBlock> {
        if !config.enabled {
            return None;
        }

        let now = Instant::now();
        let key = Self::key_digest(key);
        let state = self.lock_state();

        let counter = state.counters.get(&key)?;
        if now >= counter.reset_at || counter.failures < config.max_attempts {
            return None;
        }

        let retry_after = counter.reset_at.saturating_duration_since(now);
        Some(LoginRateLimitBlock {
            retry_after_secs: ceil_duration_seconds(retry_after),
        })
    }

    pub fn record_failure(&self, config: &RateLimitPolicy, key: &str) {
        if !config.enabled {
            return;
        }

        let now = Instant::now();
        let window = Duration::from_millis(config.window_ms.max(1));
        let key = Self::key_digest(key);
        let mut state = self.lock_state();

        if let Some(counter) = state.counters.get_mut(&key) {
            if now < counter.reset_at {
                counter.failures = counter.failures.saturating_add(1);
            } else {
                counter.failures = 1;
                counter.reset_at = deadline_after(now, window);
            }
            return;
        }

        // This is best-effort state. Evict one entry at capacity instead of
        // retaining attacker-controlled keys or scanning the full map per login.
        if state.counters.len() >= MAX_LOGIN_COUNTERS
            && let Some(victim) = state.counters.keys().next().copied()
        {
            state.counters.remove(&victim);
        }
        state.counters.insert(
            key,
            LoginAttemptCounter {
                failures: 1,
                reset_at: deadline_after(now, window),
            },
        );
    }

    pub fn clear(&self, key: &str) {
        self.lock_state().counters.remove(&Self::key_digest(key));
    }
}

fn ceil_duration_seconds(duration: Duration) -> u64 {
    let secs = duration.as_secs();
    if duration.subsec_nanos() == 0 {
        secs
    } else {
        secs.saturating_add(1)
    }
}

#[cfg(test)]
mod tests {
    use super::{LoginAttemptLimiter, MAX_LOGIN_COUNTERS, RateLimitPolicy};

    #[test]
    fn blocks_after_max_failed_attempts() {
        let limiter = LoginAttemptLimiter::new();
        let config = RateLimitPolicy {
            enabled: true,
            window_ms: 60_000,
            max_attempts: 2,
        };

        limiter.record_failure(&config, "127.0.0.1|admin");
        assert!(limiter.check(&config, "127.0.0.1|admin").is_none());

        limiter.record_failure(&config, "127.0.0.1|admin");
        let block = limiter
            .check(&config, "127.0.0.1|admin")
            .expect("limiter should block after max failures");
        assert!(block.retry_after_secs >= 1);
    }

    #[test]
    fn clear_resets_failed_attempt_counter() {
        let limiter = LoginAttemptLimiter::new();
        let config = RateLimitPolicy {
            enabled: true,
            window_ms: 60_000,
            max_attempts: 1,
        };

        limiter.record_failure(&config, "127.0.0.1|admin");
        assert!(limiter.check(&config, "127.0.0.1|admin").is_some());

        limiter.clear("127.0.0.1|admin");
        assert!(limiter.check(&config, "127.0.0.1|admin").is_none());
    }

    #[test]
    fn unique_failed_login_keys_cannot_grow_storage_past_its_cap() {
        let limiter = LoginAttemptLimiter::new();
        let config = RateLimitPolicy {
            enabled: true,
            window_ms: 60_000,
            max_attempts: 1,
        };

        for index in 0..=MAX_LOGIN_COUNTERS {
            limiter.record_failure(&config, &format!("ip|user-{index}"));
        }

        let state = limiter.lock_state();
        assert_eq!(state.counters.len(), MAX_LOGIN_COUNTERS);
        assert!(state.counters.keys().all(|key| key.len() == 32));
    }

    #[test]
    fn long_caller_keys_are_stored_as_fixed_size_digests() {
        let limiter = LoginAttemptLimiter::new();
        let config = RateLimitPolicy {
            enabled: true,
            window_ms: 60_000,
            max_attempts: 1,
        };
        let long_key = "user".repeat(100_000);

        limiter.record_failure(&config, &long_key);

        let state = limiter.lock_state();
        assert_eq!(state.counters.len(), 1);
        assert_eq!(state.counters.keys().next().unwrap().len(), 32);
    }

    #[test]
    fn oversized_window_does_not_panic_when_recording_a_failure() {
        let limiter = LoginAttemptLimiter::new();
        let config = RateLimitPolicy {
            enabled: true,
            window_ms: u64::MAX,
            max_attempts: 1,
        };

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            limiter.record_failure(&config, "127.0.0.1|admin");
        }));

        assert!(result.is_ok());
        assert_eq!(limiter.lock_state().counters.len(), 1);
    }
}
