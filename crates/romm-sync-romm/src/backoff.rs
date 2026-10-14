use romm_sync_core::{CoreError, RetryEntry, StateStore};
use romm_sync_planner::GameKey;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackoffPolicy {
    pub base_delay_ms: u64,
    pub maximum_delay_ms: u64,
    pub maximum_attempts: u32,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            base_delay_ms: 2_000,
            maximum_delay_ms: 15 * 60 * 1_000,
            maximum_attempts: 12,
        }
    }
}

impl BackoffPolicy {
    /// Returns capped exponential backoff plus deterministic per-save spread.
    /// Attempt numbers start at one.
    #[must_use]
    pub fn delay_ms(self, attempt: u32, stable_key: &str) -> Option<u64> {
        if attempt == 0 || attempt > self.maximum_attempts {
            return None;
        }
        let exponent = attempt.saturating_sub(1).min(62);
        let base = self
            .base_delay_ms
            .saturating_mul(1_u64 << exponent)
            .min(self.maximum_delay_ms);
        let spread_window = (base / 4).max(1);
        let spread = fnv1a(stable_key.as_bytes()) % spread_window;
        Some(base.saturating_add(spread).min(self.maximum_delay_ms))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RetryDisposition {
    Scheduled(RetryEntry),
    Exhausted { attempts: u32 },
}

/// Advances one durable retry using the policy's capped exponential delay.
/// Exhausted work is removed from the automatic queue so it cannot spin; the
/// caller must surface it as a permanent failure requiring user action.
///
/// # Errors
///
/// Returns an error when retry state cannot be read or written.
pub fn schedule_retry_after_failure(
    store: &StateStore,
    key: &GameKey,
    operation: &str,
    now_ms: i64,
    error: &str,
    policy: BackoffPolicy,
) -> Result<RetryDisposition, CoreError> {
    let attempts = store
        .pending_retry(key, operation)?
        .map_or(1, |entry| entry.attempt_count.saturating_add(1));
    let Some(delay) = policy.delay_ms(
        attempts,
        &format!(
            "{}:{}:{}:{operation}",
            key.platform.slug(),
            key.rom_id,
            key.slot.0
        ),
    ) else {
        store.clear_retry(key, operation)?;
        return Ok(RetryDisposition::Exhausted { attempts });
    };
    store
        .schedule_retry(key, operation, now_ms, delay, error)
        .map(RetryDisposition::Scheduled)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use romm_sync_core::StateStore;
    use romm_sync_planner::{GameKey, Platform, SlotId};

    use super::{BackoffPolicy, RetryDisposition, schedule_retry_after_failure};

    #[test]
    fn delay_is_capped_deterministic_and_finite() {
        let policy = BackoffPolicy {
            base_delay_ms: 100,
            maximum_delay_ms: 500,
            maximum_attempts: 4,
        };
        assert_eq!(
            policy.delay_ms(1, "nds:1:autosave"),
            policy.delay_ms(1, "nds:1:autosave")
        );
        assert!(policy.delay_ms(1, "a").expect("delay") >= 100);
        assert!(policy.delay_ms(4, "a").expect("delay") <= 500);
        assert_eq!(policy.delay_ms(0, "a"), None);
        assert_eq!(policy.delay_ms(5, "a"), None);
    }

    #[test]
    fn durable_schedule_advances_and_stops_at_limit() {
        let store = StateStore::open_in_memory().expect("state");
        let key = GameKey {
            platform: Platform::NintendoDs,
            rom_id: "1".to_owned(),
            slot: SlotId::from("autosave"),
        };
        let policy = BackoffPolicy {
            base_delay_ms: 100,
            maximum_delay_ms: 1_000,
            maximum_attempts: 2,
        };
        let first = schedule_retry_after_failure(&store, &key, "upload", 0, "offline", policy)
            .expect("first");
        assert!(matches!(first, RetryDisposition::Scheduled(entry) if entry.attempt_count == 1));
        let second = schedule_retry_after_failure(&store, &key, "upload", 100, "offline", policy)
            .expect("second");
        assert!(matches!(second, RetryDisposition::Scheduled(entry) if entry.attempt_count == 2));
        let third = schedule_retry_after_failure(&store, &key, "upload", 200, "offline", policy)
            .expect("third");
        assert_eq!(third, RetryDisposition::Exhausted { attempts: 3 });
        assert!(
            store
                .pending_retry(&key, "upload")
                .expect("pending")
                .is_none()
        );
    }
}
