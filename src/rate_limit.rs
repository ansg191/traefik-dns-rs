use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::{Instant, sleep, sleep_until};

pub struct RateLimit {
    capacity: u64,
    // Nano seconds of fixed window period
    period: u64,

    used: AtomicU64,

    start: u128,
    // The next reset time measured in nanoseconds from start.
    // By using a u64, we get a 584 year window before we overflow.
    reset: AtomicU64,
}

impl RateLimit {
    pub fn new(num: u64, per: Duration) -> Self {
        let start = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_nanos();

        let period = per.as_nanos()
            .try_into()
            .expect("Period is too long");
        let reset = period;

        Self {
            capacity: num,
            period,
            used: AtomicU64::new(0),
            start,
            reset: AtomicU64::new(reset),
        }
    }

    pub fn try_ready(&self) -> bool {
        if self.used.load(Ordering::Acquire) < self.capacity {
            self.used.fetch_add(1, Ordering::Release);
            true
        } else {
            // All tokens are used, check if the period has elapsed.
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_nanos();
            let elapsed = (now - self.start) as u64;

            if elapsed > self.reset.load(Ordering::Acquire) {
                // The period has elapsed, reset the counter.
                self.used.store(1, Ordering::Release);
                self.reset.fetch_add(self.period, Ordering::Release);
                true
            } else {
                false
            }
        }
    }

    pub async fn ready(&self) {
        while !self.try_ready() {
            // Unable to get a token, sleep until the next reset.
            let reset = self.reset.load(Ordering::Relaxed);
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_nanos();
            let wait_time = Duration::from_nanos((now - (self.start + reset as u128)) as u64);

            sleep(wait_time).await;
        }
    }
}
