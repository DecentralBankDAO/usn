use std::collections::HashMap;

use crate::*;

use super::cache::IntervalCache;

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[cfg_attr(test, derive(Debug, PartialEq))]
#[serde(crate = "near_sdk::serde")]
pub struct TreasuryData {
    pub reserve: HashMap<AccountId, U128>,
    pub cache: IntervalCache,
}

impl Default for TreasuryData {
    fn default() -> Self {
        Self {
            reserve: HashMap::new(),
            cache: IntervalCache::default(),
        }
    }
}
