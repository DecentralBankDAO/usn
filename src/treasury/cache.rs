use near_sdk::Timestamp;

use crate::*;

const MAX_CACHE_SIZE: usize = 8;
const FIVE_MINUTES: Timestamp = 5 * 60 * 1000_000_000;

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[cfg_attr(test, derive(Debug, PartialEq))]
#[serde(crate = "near_sdk::serde")]
pub struct CacheItem {
    pub timestamp: Timestamp,
    pub value: f64,
    pub n: u8,
}

impl CacheItem {
    /// Returns an identifier of an interval which is 5 minutes long.
    pub fn time_slot(&self) -> u64 {
        self.timestamp / FIVE_MINUTES
    }

    /// Normalized time: 5 minutes - > 1.0, -5 minutes -> -1.0, -15 minutes -> -3.
    pub fn normalized_time(&self, now: Timestamp) -> f64 {
        if self.timestamp > now {
            return (self.timestamp - now) as f64 / FIVE_MINUTES as f64;
        } else {
            return -((now - self.timestamp) as f64) / FIVE_MINUTES as f64;
        }
    }
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[cfg_attr(test, derive(Debug, PartialEq))]
#[serde(crate = "near_sdk::serde")]
pub struct IntervalCache {
    pub items: Vec<CacheItem>,
}

impl Default for IntervalCache {
    fn default() -> Self {
        Self {
            items: Vec::default(),
        }
    }
}

#[cfg_attr(test, derive(Debug, PartialEq))]
pub enum CacheError {
    NotReady,
    Gaps,
}

impl IntervalCache {
    /// Stores a new value into time-aligned evenly distributed series,
    /// 8 values in series at most.
    ///
    /// Strategy is averaging values in the 5 minute slot updating a timestamp.
    /// For example, there are measurements at some moments of time:
    /// ```text
    /// time:  00:01  00:04  00:06  00:09  00:10  00:13  00:14  00:17
    /// value:  7.2    6.9    7.1    7.4    7.9    8.1    7.8    7.5
    /// ```
    /// The cache will have these values:
    /// ```text
    /// time:         00:04         00:09                00:14  00:17
    /// value:  avg(7.2,6.9)  avg(7.1,7.4)     avg(7.9,8.1,7.8)   7.5
    /// ```
    /// effectively keeping a monotonic interval (~5 minutes) between cached values.
    ///
    pub fn append(&mut self, timestamp: Timestamp, value: f64) {
        let mut new_item = CacheItem {
            timestamp,
            value,
            n: 1,
        };

        if let Some(last_item) = self.items.last_mut() {
            if last_item.time_slot() == new_item.time_slot() {
                let n = last_item.n;
                if n < u8::MAX {
                    let value = (last_item.value * n as f64 + new_item.value) / (n as f64 + 1.);
                    new_item.value = value;
                    new_item.n += n;
                    *last_item = new_item;
                }
            } else {
                self.items.push(new_item);
            }
        } else {
            self.items.push(new_item);
        }

        if self.items.len() > MAX_CACHE_SIZE {
            self.items.remove(0);
        }
    }

    pub fn collect(&self, now: Timestamp) -> Result<(Vec<f64>, Vec<f64>), CacheError> {
        if self.items.len() < MAX_CACHE_SIZE {
            return Result::Err(CacheError::NotReady);
        }

        let mut x = Vec::<f64>::new();
        let mut y = Vec::<f64>::new();

        let mut fresh_time_slot = self.items.last().unwrap().time_slot();

        for item in self.items.iter().rev() {
            if fresh_time_slot - item.time_slot() > 1 {
                return Result::Err(CacheError::Gaps);
            }

            x.push(item.normalized_time(now));
            y.push(item.value);
            fresh_time_slot = item.time_slot();
        }

        x.reverse();
        y.reverse();

        Result::Ok((x, y))
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn test_cache_insert() {
        const ONE_MINUTE: u64 = FIVE_MINUTES / 5;
        const RANDOM_NANO_SEC: u64 = 123456;

        let mut cache = IntervalCache::default();

        cache.append(ONE_MINUTE, 7.2);
        cache.append(4 * ONE_MINUTE, 6.9);
        cache.append(6 * ONE_MINUTE, 7.1);
        cache.append(9 * ONE_MINUTE + RANDOM_NANO_SEC, 7.4);
        cache.append(10 * ONE_MINUTE, 7.9);
        cache.append(13 * ONE_MINUTE, 8.1);
        cache.append(14 * ONE_MINUTE, 7.8);
        cache.append(17 * ONE_MINUTE, 7.5);

        assert_eq!(
            vec![
                CacheItem {
                    timestamp: 4 * ONE_MINUTE,
                    value: (7.2 + 6.9) / 2.,
                    n: 2,
                },
                CacheItem {
                    timestamp: 9 * ONE_MINUTE + RANDOM_NANO_SEC,
                    value: (7.1 + 7.4) / 2.,
                    n: 2,
                },
                CacheItem {
                    timestamp: 14 * ONE_MINUTE,
                    value: (7.9 + 8.1 + 7.8) / 3.,
                    n: 3,
                },
                CacheItem {
                    timestamp: 17 * ONE_MINUTE,
                    value: 7.5,
                    n: 1,
                }
            ],
            cache.items
        );
    }

    #[test]
    fn test_cache_collect_not_ready() {
        const ONE_MINUTE: u64 = FIVE_MINUTES / 5;

        let mut cache = IntervalCache::default();

        cache.append(ONE_MINUTE, 7.2);
        cache.append(4 * ONE_MINUTE, 6.9);
        cache.append(6 * ONE_MINUTE, 7.1);
        cache.append(9 * ONE_MINUTE, 7.4);
        cache.append(10 * ONE_MINUTE, 7.9);
        cache.append(13 * ONE_MINUTE, 8.1);
        cache.append(14 * ONE_MINUTE, 7.8);
        cache.append(17 * ONE_MINUTE, 7.5);

        assert_eq!(cache.collect(18 * ONE_MINUTE), Err(CacheError::NotReady));
    }

    #[test]
    fn test_cache_collect() {
        let mut cache = IntervalCache::default();

        for i in 0..8 {
            cache.append(i * FIVE_MINUTES, 6.5);
        }

        assert_eq!(
            cache.collect(8 * FIVE_MINUTES),
            Ok((
                vec![-8.0, -7.0, -6.0, -5.0, -4.0, -3.0, -2.0, -1.0],
                vec![6.5, 6.5, 6.5, 6.5, 6.5, 6.5, 6.5, 6.5]
            ))
        );
    }
}
