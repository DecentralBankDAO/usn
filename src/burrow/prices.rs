use std::convert::TryFrom;

use super::*;
use crate::{
    oracle::{AssetOptionalPrice, Price},
    *,
};

pub struct Prices {
    prices: HashMap<TokenId, Price>,
}

impl Prices {
    pub fn new() -> Self {
        Self {
            prices: HashMap::new(),
        }
    }

    pub fn get_unwrap(&self, token_id: &TokenId) -> &Price {
        if is_usn(token_id) {
            &Price {
                multiplier: 10000, // 1:1
                decimals: 22,
            }
        } else {
            self.prices.get(token_id).expect("Asset price is missing")
        }
    }
}

impl From<PriceData> for Prices {
    fn from(data: PriceData) -> Self {
        // TODO Price is not collecting
        Self {
            prices: data
                .prices
                .into_iter()
                .filter_map(|AssetOptionalPrice { asset_id, price }| {
                    let token_id =
                        AccountId::try_from(asset_id).expect("Asset is not a valid token ID");
                    price.map(|price| (token_id, price))
                })
                .collect(),
        }
    }
}

impl Burrow {
    /// Updates last prices in the contract.
    /// The prices will only be stored if the old price for the token is already present or the
    /// asset with this token ID exists.
    pub fn internal_set_prices(&mut self, prices: &Prices) {
        for (token_id, price) in prices.prices.iter() {
            if self.last_prices.contains_key(&token_id) || self.assets.contains_key(&token_id) {
                self.last_prices.insert(token_id.clone(), price.clone());
            }
        }
    }
}
