//! Interface to `priceoracle.near`.

use near_sdk::{ext_contract, json_types::U64, Timestamp};

use crate::*;

// From https://github.com/NearDeFi/price-oracle/blob/main/src/*.rs
type AssetId = String;
type DurationSec = u32;

// From https://github.com/NearDeFi/price-oracle/blob/main/src/utils.rs
#[derive(BorshSerialize, BorshDeserialize, Deserialize, Clone, Copy)]
#[serde(crate = "near_sdk::serde")]
pub struct Price {
    pub multiplier: U128,
    pub decimals: u8,
}

// From https://github.com/NearDeFi/price-oracle/blob/main/src/asset.rs
#[derive(BorshSerialize, BorshDeserialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct AssetOptionalPrice {
    pub asset_id: AssetId,
    pub price: Option<Price>,
}

// From https://github.com/NearDeFi/price-oracle/blob/main/src/lib.rs
#[derive(BorshSerialize, BorshDeserialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct PriceData {
    timestamp: U64,
    recency_duration_sec: DurationSec,
    prices: Vec<AssetOptionalPrice>,
}

impl PriceData {
    pub fn timestamp(&self) -> Timestamp {
        Timestamp::from(self.timestamp)
    }

    pub fn recency_duration(&self) -> Timestamp {
        Timestamp::from(self.recency_duration_sec) * 10u64.pow(9)
    }

    pub fn price(&self, asset: &AssetId) -> Price {
        let asset_error = format!("Oracle has NOT provided an exchange rate for {}", asset);
        self.prices
            .iter()
            .find(|aop| &aop.asset_id == asset)
            .expect(&asset_error)
            .price
            .expect(&asset_error)
    }
}

#[ext_contract(ext_priceoracle)]
pub trait PriceOracle {
    fn get_price_data(&self, asset_ids: Vec<AssetId>) -> PriceData;
}
