//! HTTP middleware and shared request guards for VFiles.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use vfiles_config::LoginRateLimitConfig;

#[derive(Debug)]
struct LoginAttemptCounter {
    failures: u32,
    reset_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoginRateLimitBlock {
    pub retry_after_secs: u64,
}

#[derive(Debug, Default)]
pub struct LoginAttemptLimiter {
    counters: Mutex<HashMap<String, LoginAttemptCounter>>,
}

impl LoginAttemptLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check(&self, config: &LoginRateLimitConfig, key: &str) -> Option<LoginRateLimitBlock> {
        if !config.enabled {
            return None;
        }

        let now = Instant::now();
        let mut counters = self.counters.lock().expect("login limiter lock poisoned");
        Self::prune_expired(&mut counters, now);

        let counter = counters.get(key)?;
        if now >= counter.reset_at || counter.failures < config.max_attempts {
            return None;
        }

        let retry_after = counter.reset_at.saturating_duration_since(now);
        Some(LoginRateLimitBlock {
            retry_after_secs: ceil_duration_seconds(retry_after),
        })
    }

    pub fn record_failure(&self, config: &LoginRateLimitConfig, key: &str) {
        if !config.enabled {
            return;
        }

        let now = Instant::now();
        let window = Duration::from_millis(config.window_ms.max(1));
        let mut counters = self.counters.lock().expect("login limiter lock poisoned");

        match counters.get_mut(key) {
            Some(counter) if now < counter.reset_at => {
                counter.failures = counter.failures.saturating_add(1);
            }
            Some(counter) => {
                counter.failures = 1;
                counter.reset_at = now + window;
            }
            None => {
                counters.insert(
                    key.to_string(),
                    LoginAttemptCounter {
                        failures: 1,
                        reset_at: now + window,
                    },
                );
            }
        }

        Self::prune_if_oversized(&mut counters, now);
    }

    pub fn clear(&self, key: &str) {
        let mut counters = self.counters.lock().expect("login limiter lock poisoned");
        counters.remove(key);
    }

    fn prune_expired(counters: &mut HashMap<String, LoginAttemptCounter>, now: Instant) {
        counters.retain(|_, counter| now < counter.reset_at);
    }

    fn prune_if_oversized(counters: &mut HashMap<String, LoginAttemptCounter>, now: Instant) {
        if counters.len() > 50_000 {
            Self::prune_expired(counters, now);
        }
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
    use super::LoginAttemptLimiter;
    use vfiles_config::LoginRateLimitConfig;

    #[test]
    fn blocks_after_max_failed_attempts() {
        let limiter = LoginAttemptLimiter::new();
        let config = LoginRateLimitConfig {
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
        let config = LoginRateLimitConfig {
            enabled: true,
            window_ms: 60_000,
            max_attempts: 1,
        };

        limiter.record_failure(&config, "127.0.0.1|admin");
        assert!(limiter.check(&config, "127.0.0.1|admin").is_some());

        limiter.clear("127.0.0.1|admin");
        assert!(limiter.check(&config, "127.0.0.1|admin").is_none());
    }
}
