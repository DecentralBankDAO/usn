use near_sdk::Timestamp;

use crate::*;

const MAX_CACHE_SIZE: usize = 6;
const FIVE_MINUTES: Timestamp = 5 * 60 * 1000_000_000;
const ALFA_BASE: f64 = 0.5;

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[cfg_attr(test, derive(Debug, PartialEq))]
#[serde(crate = "near_sdk::serde")]
pub struct CacheItem {
    pub timestamp: Timestamp,
    pub value: f64,
    pub smoothed_value: f64,
    pub n: u8,
}

impl CacheItem {
    pub fn new(timestamp: Timestamp, value: f64) -> Self {
        CacheItem {
            timestamp,
            value,
            smoothed_value: value,
            n: 1,
        }
    }

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
    NotRecent,
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
        let mut new_item = CacheItem::new(timestamp, value);

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

        if self.items.len() > 1 {
            let last_item = self.items[self.items.len() - 2].clone();
            let mut new_item = self.items.last_mut().unwrap();

            // Get the distance between normalized time-steps
            let time_distance = new_item.normalized_time(last_item.timestamp);

            let alfa = ALFA_BASE.powf(1. / time_distance as f64);

            // Make the exponential smoothing of exchange rates
            new_item.smoothed_value =
                last_item.smoothed_value * (1. - alfa) + new_item.value * alfa;
        }

        if self.items.len() > MAX_CACHE_SIZE {
            self.items.remove(0);
        }
    }

    pub fn collect(&self, now: Timestamp) -> Result<(Vec<f64>, Vec<f64>, Vec<f64>), CacheError> {
        if self.items.len() < MAX_CACHE_SIZE {
            return Result::Err(CacheError::NotReady);
        }

        let mut x = Vec::<f64>::new();
        let mut y = Vec::<f64>::new();
        let mut er = Vec::<f64>::new();

        let mut fresh_time_slot = self.items.last().unwrap().time_slot();

        let now_time_slot = CacheItem::new(now, 0.).time_slot();

        if now_time_slot - fresh_time_slot > 1 {
            return Result::Err(CacheError::NotRecent);
        }

        // Define the algorithm parameters
        for item in self.items.iter().rev() {
            if fresh_time_slot - item.time_slot() > 1 {
                return Result::Err(CacheError::Gaps);
            }

            x.push(item.normalized_time(now));
            y.push(item.smoothed_value);
            er.push(item.value);
            fresh_time_slot = item.time_slot();
        }

        x.reverse();
        y.reverse();
        er.reverse();

        Result::Ok((x, y, er))
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn test_cache_insert() {
        const ONE_MINUTE: u64 = FIVE_MINUTES / 5;

        let mut cache = IntervalCache::default();

        cache.append(7 * ONE_MINUTE, 3.085);
        cache.append(11 * ONE_MINUTE, 5.234);
        cache.append(12 * ONE_MINUTE, 5.011);
        cache.append(17 * ONE_MINUTE, 6.656);
        cache.append(21 * ONE_MINUTE, 6.813);
        cache.append(22 * ONE_MINUTE, 7.613);
        cache.append(23 * ONE_MINUTE, 8.141);
        cache.append(30 * ONE_MINUTE, 9.518);
        cache.append(35 * ONE_MINUTE, 10.813);

        assert_eq!(
            vec![
                CacheItem {
                    timestamp: 7 * ONE_MINUTE,
                    value: 3.085,
                    smoothed_value: 3.085,
                    n: 1,
                },
                CacheItem {
                    timestamp: 12 * ONE_MINUTE,
                    value: (5.011 + 5.234) / 2.,
                    smoothed_value: 4.10375,
                    n: 2,
                },
                CacheItem {
                    timestamp: 17 * ONE_MINUTE,
                    value: 6.656,
                    smoothed_value: 5.379875,
                    n: 1,
                },
                CacheItem {
                    timestamp: 23 * ONE_MINUTE,
                    value: (6.813 + 7.613 + 8.141) / 3.,
                    smoothed_value: 6.582289084625408,
                    n: 3,
                },
                CacheItem {
                    timestamp: 30 * ONE_MINUTE,
                    value: 9.518,
                    smoothed_value: 8.371624929944781,
                    n: 1,
                },
                CacheItem {
                    timestamp: 35 * ONE_MINUTE,
                    value: 10.813,
                    smoothed_value: 9.592312464972391,
                    n: 1,
                }
            ],
            cache.items
        );

        cache.append(42 * ONE_MINUTE, 12.046);

        assert_eq!(
            &CacheItem {
                timestamp: 42 * ONE_MINUTE,
                value: 12.046,
                smoothed_value: 11.08785176914738,
                n: 1,
            },
            cache.items.last().unwrap()
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

        cache.append(FIVE_MINUTES, 3.085);
        cache.append(2 * FIVE_MINUTES, 5.011);
        cache.append(3 * FIVE_MINUTES, 6.656);
        cache.append(4 * FIVE_MINUTES, 8.140);
        cache.append(5 * FIVE_MINUTES, 9.518);
        cache.append(6 * FIVE_MINUTES, 10.813);

        assert_eq!(
            cache.collect(7 * FIVE_MINUTES),
            Ok((
                vec![-6.0, -5.0, -4.0, -3.0, -2.0, -1.0],
                vec![3.085, 4.048, 5.352, 6.746, 8.132000000000001, 9.4725],
                vec![3.085, 5.011, 6.656, 8.140, 9.518, 10.813],
            ))
        );

        assert_eq!(cache.collect(9 * FIVE_MINUTES), Err(CacheError::NotRecent));
    }
}
