use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tracing::{info, warn};

use crate::config::DownstreamKeyConfig;

pub struct KeyPool {
    keys: RwLock<Vec<Arc<PooledKey>>>,
    state: Mutex<PoolState>,
}

struct PooledKey {
    key: String,
    weight: AtomicU32,
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
}

impl KeyPool {
    pub fn new(configs: &[DownstreamKeyConfig]) -> Self {
        let keys = configs
            .iter()
            .map(|c| {
                Arc::new(PooledKey {
                    key: c.key.clone(),
                    weight: AtomicU32::new(c.weight),
                    healthy: AtomicBool::new(true),
                    consecutive_errors: AtomicU32::new(0),
                    request_count: AtomicU32::new(0),
                })
            })
            .collect();

        Self {
            keys: RwLock::new(keys),
            state: Mutex::new(PoolState {
                round_robin_index: 0,
            }),
        }
    }

    pub fn acquire_key(&self) -> Result<AcquiredKey, crate::error::AppError> {
        let mut state = self.state.lock().unwrap();
        let keys = self.keys.read().unwrap();

        let healthy_keys: Vec<&Arc<PooledKey>> = keys
            .iter()
            .filter(|k| k.healthy.load(Ordering::Relaxed))
            .collect();

        if healthy_keys.is_empty() {
            return Err(crate::error::AppError::NoKeysAvailable);
        }

        let total_weight: u32 = healthy_keys.iter().map(|k| k.weight.load(Ordering::Relaxed)).sum();
        if total_weight == 0 {
            return Err(crate::error::AppError::NoKeysAvailable);
        }

        let index = state.round_robin_index % total_weight as usize;
        state.round_robin_index = (state.round_robin_index + 1) % total_weight as usize;

        let mut accumulated = 0u32;
        for key in &healthy_keys {
            accumulated += key.weight.load(Ordering::Relaxed);
            if (accumulated as usize) > index {
                key.request_count.fetch_add(1, Ordering::Relaxed);
                return Ok(AcquiredKey {
                    key: key.key.clone(),
                });
            }
        }

        let key = healthy_keys[0];
        key.request_count.fetch_add(1, Ordering::Relaxed);
        Ok(AcquiredKey {
            key: key.key.clone(),
        })
    }

    pub fn report_success(&self, acquired: &AcquiredKey) {
        let keys = self.keys.read().unwrap();
        if let Some(key) = keys.iter().find(|k| k.key == acquired.key) {
            key.consecutive_errors.store(0, Ordering::Relaxed);
        }
    }

    pub fn report_failure(&self, acquired: &AcquiredKey) {
        let keys = self.keys.read().unwrap();
        if let Some(key) = keys.iter().find(|k| k.key == acquired.key) {
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

    pub fn add_key(&self, key: String, weight: u32) {
        let mut keys = self.keys.write().unwrap();
        if let Some(existing) = keys.iter().find(|k| k.key == key) {
            existing.weight.store(weight, Ordering::Relaxed);
            existing.healthy.store(true, Ordering::Relaxed);
            existing.consecutive_errors.store(0, Ordering::Relaxed);
        } else {
            keys.push(Arc::new(PooledKey {
                key,
                weight: AtomicU32::new(weight),
                healthy: AtomicBool::new(true),
                consecutive_errors: AtomicU32::new(0),
                request_count: AtomicU32::new(0),
            }));
        }
    }

    pub fn remove_key(&self, key: &str) {
        let mut keys = self.keys.write().unwrap();
        keys.retain(|k| k.key != key);
    }

    pub fn key_count(&self) -> usize {
        let keys = self.keys.read().unwrap();
        keys.len()
    }

    pub fn healthy_key_count(&self) -> usize {
        let keys = self.keys.read().unwrap();
        keys.iter()
            .filter(|k| k.healthy.load(Ordering::Relaxed))
            .count()
    }

    pub fn key_stats(&self) -> Vec<KeyStats> {
        let keys = self.keys.read().unwrap();
        keys.iter()
            .map(|k| KeyStats {
                key: crate::config::Config::mask_key(&k.key),
                actual_key: k.key.clone(),
                weight: k.weight.load(Ordering::Relaxed),
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
    pub actual_key: String,
    pub weight: u32,
    pub healthy: bool,
    pub request_count: u32,
    pub consecutive_errors: u32,
}
