use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

use crate::config::DownstreamKeyConfig;

pub struct KeyPool {
    keys: Vec<Arc<PooledKey>>,
    state: Mutex<PoolState>,
}

struct PooledKey {
    key: String,
    weight: u32,
    healthy: AtomicBool,
    consecutive_errors: AtomicU32,
    request_count: AtomicU32,
}

struct PoolState {
    round_robin_index: usize,
}

#[derive(Debug, Clone)]
pub struct AcquiredKey {
    pub key: String,
    pub index: usize,
}

impl KeyPool {
    pub fn new(configs: &[DownstreamKeyConfig]) -> Self {
        let keys = configs
            .iter()
            .map(|c| {
                Arc::new(PooledKey {
                    key: c.key.clone(),
                    weight: c.weight,
                    healthy: AtomicBool::new(true),
                    consecutive_errors: AtomicU32::new(0),
                    request_count: AtomicU32::new(0),
                })
            })
            .collect();

        Self {
            keys,
            state: Mutex::new(PoolState {
                round_robin_index: 0,
            }),
        }
    }

    pub fn acquire_key(&self) -> Result<AcquiredKey, crate::error::AppError> {
        let mut state = self.state.lock().unwrap();

        let healthy_keys: Vec<(usize, &Arc<PooledKey>)> = self
            .keys
            .iter()
            .enumerate()
            .filter(|(_, k)| k.healthy.load(Ordering::Relaxed))
            .collect();

        if healthy_keys.is_empty() {
            return Err(crate::error::AppError::NoKeysAvailable);
        }

        let total_weight: u32 = healthy_keys.iter().map(|(_, k)| k.weight).sum();
        let index = state.round_robin_index % total_weight as usize;
        state.round_robin_index = (state.round_robin_index + 1) % total_weight as usize;

        let mut accumulated = 0u32;
        for (i, key) in &healthy_keys {
            accumulated += key.weight;
            if (accumulated as usize) > index {
                key.request_count.fetch_add(1, Ordering::Relaxed);
                return Ok(AcquiredKey {
                    key: key.key.clone(),
                    index: *i,
                });
            }
        }

        let (i, key) = healthy_keys[0];
        key.request_count.fetch_add(1, Ordering::Relaxed);
        Ok(AcquiredKey {
            key: key.key.clone(),
            index: i,
        })
    }

    pub fn report_success(&self, acquired: &AcquiredKey) {
        if let Some(key) = self.keys.get(acquired.index) {
            key.consecutive_errors.store(0, Ordering::Relaxed);
        }
    }

    pub fn report_failure(&self, acquired: &AcquiredKey) {
        if let Some(key) = self.keys.get(acquired.index) {
            let errors = key.consecutive_errors.fetch_add(1, Ordering::Relaxed) + 1;
            if errors >= 3 {
                key.healthy.store(false, Ordering::Relaxed);
                warn!(
                    key = %crate::config::Config::mask_key(&key.key),
                    errors = errors,
                    "Key marked unhealthy, will recover in 60s"
                );

                let key_clone = Arc::clone(key);
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    key_clone.healthy.store(true, Ordering::Relaxed);
                    key_clone.consecutive_errors.store(0, Ordering::Relaxed);
                    info!(
                        key = %crate::config::Config::mask_key(&key_clone.key),
                        "Key recovered after cooldown"
                    );
                });
            }
        }
    }

    pub fn key_count(&self) -> usize {
        self.keys.len()
    }

    pub fn healthy_key_count(&self) -> usize {
        self.keys
            .iter()
            .filter(|k| k.healthy.load(Ordering::Relaxed))
            .count()
    }

    pub fn key_stats(&self) -> Vec<KeyStats> {
        self.keys
            .iter()
            .map(|k| KeyStats {
                key: crate::config::Config::mask_key(&k.key),
                weight: k.weight,
                healthy: k.healthy.load(Ordering::Relaxed),
                request_count: k.request_count.load(Ordering::Relaxed),
                consecutive_errors: k.consecutive_errors.load(Ordering::Relaxed),
            })
            .collect()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct KeyStats {
    pub key: String,
    pub weight: u32,
    pub healthy: bool,
    pub request_count: u32,
    pub consecutive_errors: u32,
}
