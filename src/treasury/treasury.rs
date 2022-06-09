use crate::*;

use super::cache::IntervalCache;

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[cfg_attr(test, derive(Debug, PartialEq))]
#[serde(crate = "near_sdk::serde")]
pub struct TreasuryData {
    pub cache: IntervalCache,
}

impl Default for TreasuryData {
    fn default() -> Self {
        Self {
            cache: IntervalCache::default(),
        }
    }
}
