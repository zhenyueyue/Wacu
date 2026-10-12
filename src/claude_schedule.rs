//! Claude automatic refresh only runs while a local Code session is working.
use std::time::{Duration, Instant};

pub const INTERVAL: Duration = Duration::from_secs(300);
pub const MANUAL_COOLDOWN: Duration = Duration::from_secs(60);

pub struct Schedule {
    next_auto: Instant,
    next_manual: Instant,
}
impl Schedule {
    pub fn new(now: Instant) -> Self {
        Self {
            next_auto: now,
            next_manual: now,
        }
    }
    pub fn due(&self, now: Instant, manual: bool, enabled: bool, working: bool) -> bool {
        if manual {
            now >= self.next_manual
        } else {
            enabled && working && now >= self.next_auto
        }
    }
    pub fn begin(&mut self, now: Instant) {
        self.next_auto = now + INTERVAL;
        self.next_manual = now + MANUAL_COOLDOWN;
    }
    pub fn complete(&mut self, now: Instant, delay: Duration, limited: bool) {
        self.next_auto = self.next_auto.max(now + delay);
        if limited {
            self.next_manual = self.next_manual.max(now + delay);
        }
    }
    pub fn cooldown_seconds(&self, now: Instant) -> i32 {
        let left = self.next_manual.saturating_duration_since(now);
        left.as_secs()
            .saturating_add(u64::from(left.subsec_nanos() > 0))
            .min(i32::MAX as u64) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_never_polls_and_work_resumes_without_bypassing_spacing() {
        let t = Instant::now();
        let mut s = Schedule::new(t);
        assert!(!s.due(t, false, true, false));
        assert!(s.due(t, false, true, true));
        s.begin(t);
        s.complete(t + Duration::from_secs(3), INTERVAL, false);
        assert!(!s.due(t + Duration::from_secs(200), false, true, true));
        assert!(!s.due(t + Duration::from_secs(900), false, true, false));
        assert!(s.due(t + Duration::from_secs(900), false, true, true));
        assert!(!s.due(t + Duration::from_secs(900), false, false, true));
    }
    #[test]
    fn manual_cooldown_and_rate_limit_apply_even_when_idle() {
        let t = Instant::now();
        let mut s = Schedule::new(t);
        s.begin(t);
        assert_eq!(s.cooldown_seconds(t), 60);
        assert!(!s.due(t + Duration::from_secs(59), true, true, false));
        assert!(s.due(t + Duration::from_secs(60), true, true, false));
        s.complete(t + Duration::from_secs(2), Duration::from_secs(900), true);
        assert!(!s.due(t + Duration::from_secs(901), true, true, true));
        assert!(s.due(t + Duration::from_secs(902), true, true, false));
        assert!(!s.due(t + Duration::from_secs(901), false, true, true));
    }
}
