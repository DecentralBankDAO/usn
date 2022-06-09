use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::Timestamp;

use crate::*;

pub const ONE_MINUTE: Timestamp = 1 * 60 * 1000_000_000;
pub const FIVE_MINUTES: Timestamp = 5 * ONE_MINUTE;
pub const ONE_HOUR: Timestamp = 60 * ONE_MINUTE;

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct VolumeCacheItem {
    usn: u128,
    near: u128,
    timestamp: Timestamp,
}

impl VolumeCacheItem {
    pub fn new(usn: u128, near: u128, ts: Timestamp) -> VolumeCacheItem {
        VolumeCacheItem {
            usn: usn,
            near: near,
            timestamp: ts,
        }
    }

    pub fn timestamp(&self) -> Timestamp {
        return self.timestamp;
    }

    pub fn add(&mut self, usn: u128, near: u128) {
        self.usn += usn;
        self.near += near;
    }

    pub fn usn(&self) -> u128 {
        self.usn
    }

    pub fn near(&self) -> u128 {
        self.near
    }
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Default)]
#[serde(crate = "near_sdk::serde")]
pub struct VolumeCache {
    time_slot: Timestamp,
    max_age: Timestamp,
    sum_usn: u128,
    sum_near: u128,
    items: Vec<VolumeCacheItem>,
}

impl VolumeCache {
    pub fn new(time_slot: Timestamp, max_age: Timestamp) -> Self {
        debug_assert!(time_slot < max_age);

        VolumeCache {
            time_slot: time_slot,
            max_age: max_age,
            sum_usn: 0,
            sum_near: 0,
            items: Vec::default(),
        }
    }

    pub fn new_1hour() -> Self {
        VolumeCache::new(FIVE_MINUTES, ONE_HOUR)
    }

    pub fn new_5min() -> Self {
        VolumeCache::new(ONE_MINUTE, FIVE_MINUTES)
    }

    fn remove_older_items(&mut self, now: Timestamp) {
        let max_age = self.max_age;
        let mut removed_usn_sum: u128 = 0;
        let mut removed_near_sum: u128 = 0;

        self.items.retain(|x| {
            let time_diff = now - x.timestamp();
            if time_diff > 0 && time_diff > max_age {
                removed_usn_sum += x.usn();
                removed_near_sum += x.near();
                return false;
            }

            return true;
        });

        self.sum_near -= removed_near_sum;
        self.sum_usn -= removed_usn_sum;
    }

    pub fn add(&mut self, usn: u128, near: u128, now: Timestamp) {
        let item = VolumeCacheItem::new(usn, near, now);

        if let Some(last_item) = self.items.last_mut() {
            if now >= last_item.timestamp() {
                let time_diff = now - last_item.timestamp();
                if time_diff > self.time_slot {
                    self.items.push(item);
                } else {
                    last_item.add(usn, near);
                }
                self.sum_near += near;
                self.sum_usn += usn;
            }
        } else {
            self.items.push(item);
            self.sum_near += near;
            self.sum_usn += usn;
        }
        self.remove_older_items(now);
    }

    pub fn sum_usn(&self) -> u128 {
        self.sum_usn
    }

    pub fn sum_near(&self) -> u128 {
        self.sum_near
    }

    #[cfg(test)]
    pub fn items(&self) -> &Vec<VolumeCacheItem> {
        &self.items
    }
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Default)]
#[serde(crate = "near_sdk::serde")]
pub struct MinMaxRate {
    max_previous: Option<ExchangeRateValue>,
    max_current: Option<ExchangeRateValue>,
    min_previous: Option<ExchangeRateValue>,
    min_current: Option<ExchangeRateValue>,
    timestamp: Timestamp,
}

impl MinMaxRate {
    pub fn update(&mut self, rates: &ExchangeRates, ts: Timestamp) {
        let time_diff = ts - self.timestamp;
        let max_rate = ExchangeRateValue::from(rates.max());
        let min_rate = ExchangeRateValue::from(rates.min());
        if time_diff < FIVE_MINUTES {
            if self.max_current.is_none() || max_rate > self.max_current.unwrap() {
                self.max_current = Some(max_rate);
            }
            if self.min_current.is_none() || min_rate < self.min_current.unwrap() {
                self.min_current = Some(min_rate);
            }
        } else {
            self.timestamp = ts;
            self.max_previous = self.max_current;
            self.max_current = Some(max_rate);
            self.min_previous = self.min_current;
            self.min_current = Some(min_rate);
        }
    }

    pub fn max_previous(&self) -> Option<ExchangeRateValue> {
        if self.max_previous.is_none() {
            self.max_current
        } else {
            self.max_previous
        }
    }

    pub fn min_previous(&self) -> Option<ExchangeRateValue> {
        if self.min_previous.is_none() {
            self.min_current
        } else {
            self.min_previous
        }
    }
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Default)]
#[serde(crate = "near_sdk::serde")]
pub struct VolumeHistory {
    pub one_hour: VolumeCache,
    pub five_min: VolumeCache,
}

impl VolumeHistory {
    pub fn new() -> Self {
        VolumeHistory {
            one_hour: VolumeCache::new_1hour(),
            five_min: VolumeCache::new_5min(),
        }
    }

    pub fn add(&mut self, usn: u128, near: u128, now: Timestamp) {
        self.one_hour.add(usn, near, now);
        self.five_min.add(usn, near, now);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use crate::history::{MinMaxRate, VolumeCache, FIVE_MINUTES, ONE_HOUR};
    #[test]
    fn test_volume_cache() {
        let mut cache = VolumeCache::new_1hour();
        let timestamp = ONE_HOUR;

        assert_eq!(cache.sum_usn(), 0);
        assert_eq!(cache.sum_near(), 0);

        cache.add(1, 2, timestamp);
        cache.add(2, 3, timestamp);
        assert_eq!(cache.sum_usn(), 3);
        assert_eq!(cache.sum_near(), 5);
        assert_eq!(cache.items().len(), 1);

        cache.add(4, 0, timestamp + FIVE_MINUTES);
        cache.add(5, 0, timestamp + FIVE_MINUTES * 2);
        assert_eq!(cache.sum_usn(), 12);
        assert_eq!(cache.items().len(), 2);
        assert_eq!(cache.items()[0].usn(), 7);
        assert_eq!(cache.items()[1].usn(), 5);

        cache.add(1, 0, timestamp + FIVE_MINUTES * 3 + 1);
        assert_eq!(cache.sum_usn(), 13);
        assert_eq!(cache.items().len(), 3);

        cache.add(1, 0, timestamp + ONE_HOUR);
        assert_eq!(cache.sum_usn(), 14);
        assert_eq!(cache.items().len(), 4);

        cache.add(1, 0, timestamp + ONE_HOUR + 1);
        assert_eq!(cache.sum_usn(), 8);
        assert_eq!(cache.items().len(), 3);
    }

    #[test]
    fn test_min_max_rate() {
        use crate::env;
        use crate::oracle::{ExchangeRate, ExchangeRateValue, ExchangeRates};

        let mut rate_history = MinMaxRate::default();

        assert!(rate_history.max_previous().is_none());
        assert!(rate_history.min_previous().is_none());

        let current = ExchangeRate::test_create_rate(520944008, 32);
        let smooth = ExchangeRate::test_create_rate(520944007, 32);
        let rates = ExchangeRates::new(current, smooth);
        rate_history.update(&rates, env::block_timestamp());
        assert!(rate_history.max_previous().unwrap() == ExchangeRateValue::from(current));
        assert!(rate_history.min_previous().unwrap() == ExchangeRateValue::from(smooth));

        let current = ExchangeRate::test_create_rate(520944009, 32);
        let smooth = ExchangeRate::test_create_rate(520944006, 32);
        let rates = ExchangeRates::new(current, smooth);
        rate_history.update(&rates, env::block_timestamp() + 1);
        assert!(rate_history.max_previous().unwrap() == ExchangeRateValue::from(current));
        assert!(rate_history.min_previous().unwrap() == ExchangeRateValue::from(smooth));

        let current1 = ExchangeRate::test_create_rate(520944010, 32);
        let smooth1 = ExchangeRate::test_create_rate(520944011, 32);
        let rates = ExchangeRates::new(current1, smooth1);
        rate_history.update(&rates, env::block_timestamp() + FIVE_MINUTES + 1);
        assert!(rate_history.max_previous().unwrap() == ExchangeRateValue::from(current));
        assert!(rate_history.min_previous().unwrap() == ExchangeRateValue::from(smooth));

        let current2 = ExchangeRate::test_create_rate(520944010, 32);
        let smooth2 = ExchangeRate::test_create_rate(520944011, 32);
        let rates = ExchangeRates::new(current2, smooth2);
        rate_history.update(&rates, env::block_timestamp() + FIVE_MINUTES * 3 + 1);
        assert!(rate_history.max_previous().unwrap() == ExchangeRateValue::from(smooth1));
        assert!(rate_history.min_previous().unwrap() == ExchangeRateValue::from(current1));
    }
}
