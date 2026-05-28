use dashmap::DashMap;
use std::time::Instant;

pub struct RateLimiter {
    buckets: DashMap<String, TokenBucket>,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
}

struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

impl RateLimiter {
    pub fn new(requests_per_minute: u32, burst_size: u32) -> Self {
        Self {
            buckets: DashMap::new(),
            max_tokens: burst_size as f64,
            refill_rate: requests_per_minute as f64 / 60.0,
        }
    }

    /// Returns Ok(remaining_tokens) if allowed, Err(retry_after_secs) if rate limited.
    pub fn check(&self, key: &str) -> Result<u32, f64> {
        let now = Instant::now();

        let mut bucket = self.buckets.entry(key.to_string()).or_insert_with(|| {
            TokenBucket {
                tokens: self.max_tokens,
                last_refill: now,
            }
        });

        // Refill tokens based on elapsed time
        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        bucket.last_refill = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(bucket.tokens as u32)
        } else {
            let retry_after = (1.0 - bucket.tokens) / self.refill_rate;
            Err(retry_after)
        }
    }

    pub fn cleanup_stale(&self) {
        let cutoff = Instant::now() - std::time::Duration::from_secs(3600);
        self.buckets.retain(|_, bucket| bucket.last_refill > cutoff);
    }
}
