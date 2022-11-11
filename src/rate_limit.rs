use std::ops::Add;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

pub struct RateLimit {
    // The number of requests that can be made in the time period.
    capacity: u64,
    // Milliseconds of fixed window period.
    period: u64,

    // Used requests in current window.
    used: AtomicU64,
    // The next reset time measured in milliseconds from UNIX_EPOCH.
    reset: AtomicU64,
}

impl RateLimit {
    pub fn new(num: u64, per: Duration) -> Self {
        let period = per.as_millis()
            .try_into()
            .expect("Period is too long");
        let reset = SystemTime::now()
            .add(per)
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis()
            .try_into()
            .expect("Period is too long");

        Self {
            capacity: num,
            period,
            used: AtomicU64::new(0),
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
                .as_millis() as u64;

            if now > self.reset.load(Ordering::Acquire) {
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
                .as_millis() as u64;
            let wait_time = Duration::from_millis(reset - now);

            sleep(wait_time).await;
        }
    }
}
