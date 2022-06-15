use near_sdk::Timestamp;
use partial_min_max;

use crate::oracle::priceoracle::{ext_priceoracle, PriceData};
use crate::*;

pub const DEFAULT_RATE_DECIMALS: u8 = 28;

struct OracleConfig {
    pub oracle_address: &'static str,
    pub asset_id: &'static str,
    pub smooth_asset_id: &'static str,
    pub gas: Gas,
}

const CONFIG: OracleConfig = if cfg!(feature = "mainnet") {
    OracleConfig {
        oracle_address: "priceoracle.near",
        asset_id: "wrap.near",             // NEARUSDT
        smooth_asset_id: "wrap.near#3600", // 1 hour EMA for NEARUSDT
        gas: Gas(5_000_000_000_000),
    }
} else if cfg!(feature = "testnet") {
    OracleConfig {
        oracle_address: "priceoracle.testnet",
        asset_id: "wrap.testnet",             // NEARUSDT
        smooth_asset_id: "wrap.testnet#3600", // 1 hour EMA for NEARUSDT
        gas: Gas(5_000_000_000_000),
    }
} else {
    OracleConfig {
        oracle_address: "priceoracle.test.near",
        asset_id: "wrap.test.near",
        smooth_asset_id: "wrap.test.near#3600",
        gas: Gas(5_000_000_000_000),
    }
};

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct ExchangeRates {
    pub current: ExchangeRate,
    pub smooth: ExchangeRate,
}

impl ExchangeRates {
    pub fn max(&self) -> ExchangeRate {
        partial_min_max::max(self.current, self.smooth)
    }

    pub fn min(&self) -> ExchangeRate {
        partial_min_max::min(self.current, self.smooth)
    }

    pub fn new(current: ExchangeRate, smooth: ExchangeRate) -> Self {
        ExchangeRates {
            current: current,
            smooth: smooth,
        }
    }
}

fn compare_rates(
    multiplier1: u128,
    decimals1: u8,
    multiplier2: u128,
    decimals2: u8,
) -> std::cmp::Ordering {
    let mut self_mult = multiplier1;
    let mut other_mult = multiplier2;

    if decimals2 > decimals1 {
        let exp = u32::from(decimals2) - u32::from(decimals1);
        self_mult = multiplier1 * 10u128.pow(exp);
    } else {
        let exp = u32::from(decimals1) - u32::from(decimals2);
        other_mult = multiplier2 * 10u128.pow(exp);
    }

    self_mult.cmp(&other_mult)
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Copy, Default, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct ExchangeRate {
    multiplier: u128,
    decimals: u8,
    timestamp: Timestamp,
    recency_duration: Timestamp,
}

impl ExchangeRate {
    pub fn new(multiplier: u128, decimals: u8) -> Self {
        Self {
            multiplier: multiplier,
            decimals: decimals,
            timestamp: env::block_timestamp(),
            recency_duration: 0,
        }
    }

    pub fn multiplier(&self) -> u128 {
        self.multiplier
    }

    pub fn decimals(&self) -> u8 {
        self.decimals
    }

    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    pub fn to_decimals(&self, decimals: u8) -> ExchangeRate {
        let mut multiplier = self.multiplier;
        if self.decimals < decimals {
            let exp = decimals - self.decimals;
            multiplier = self.multiplier * 10u128.pow(u32::from(exp));
        } else if self.decimals > decimals {
            let exp = self.decimals - decimals;
            multiplier = self.multiplier / 10u128.pow(u32::from(exp))
        }

        ExchangeRate::new(multiplier, decimals)
    }
}

impl PartialOrd for ExchangeRate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(compare_rates(
            self.multiplier(),
            self.decimals(),
            other.multiplier(),
            other.decimals(),
        ))
    }
}

impl PartialEq for ExchangeRate {
    fn eq(&self, other: &Self) -> bool {
        compare_rates(
            self.multiplier(),
            self.decimals(),
            other.multiplier(),
            other.decimals(),
        ) == std::cmp::Ordering::Equal
    }
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct ExchangeRateValue {
    multiplier: U128,
    decimals: u8,
}

impl ExchangeRateValue {
    pub fn new(multiplier: u128, decimals: u8) -> Self {
        Self {
            multiplier: U128::from(multiplier),
            decimals: decimals,
        }
    }

    pub fn multiplier(&self) -> u128 {
        self.multiplier.0
    }

    pub fn decimals(&self) -> u8 {
        self.decimals
    }
}

impl From<ExchangeRate> for ExchangeRateValue {
    fn from(rate: ExchangeRate) -> Self {
        Self {
            multiplier: U128::from(rate.multiplier()),
            decimals: rate.decimals(),
        }
    }
}

impl From<ExchangeRateValue> for ExchangeRate {
    fn from(rate: ExchangeRateValue) -> Self {
        ExchangeRate::new(u128::from(rate.multiplier), rate.decimals)
    }
}

impl PartialOrd for ExchangeRateValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(compare_rates(
            self.multiplier(),
            self.decimals(),
            other.multiplier(),
            other.decimals(),
        ))
    }
}

impl PartialEq for ExchangeRateValue {
    fn eq(&self, other: &Self) -> bool {
        compare_rates(
            self.multiplier(),
            self.decimals(),
            other.multiplier(),
            other.decimals(),
        ) == std::cmp::Ordering::Equal
    }
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct Oracle {
    pub last_report: Option<ExchangeRates>,
}

impl Default for Oracle {
    fn default() -> Self {
        Self { last_report: None }
    }
}

impl Oracle {
    pub fn get_exchange_rate_promise() -> Promise {
        ext_priceoracle::get_price_data(
            vec![CONFIG.asset_id.into(), CONFIG.smooth_asset_id.into()],
            CONFIG.oracle_address.parse().unwrap(),
            0,
            CONFIG.gas,
        )
    }
}

impl From<PriceData> for ExchangeRates {
    fn from(price_data: PriceData) -> Self {
        if env::block_timestamp() >= price_data.timestamp() + price_data.recency_duration() {
            env::panic_str("Oracle provided an outdated price data");
        }

        let current_price = price_data.price(&CONFIG.asset_id.into());
        let smooth_price = price_data.price(&CONFIG.smooth_asset_id.into());

        let current_rate = ExchangeRate {
            multiplier: current_price.multiplier.into(),
            decimals: current_price.decimals,
            timestamp: price_data.timestamp(),
            recency_duration: price_data.recency_duration(),
        };

        let smooth_rate = ExchangeRate {
            multiplier: smooth_price.multiplier.into(),
            decimals: smooth_price.decimals.into(),
            timestamp: price_data.timestamp(),
            recency_duration: price_data.recency_duration(),
        };

        ExchangeRates {
            current: current_rate,
            smooth: smooth_rate,
        }
    }
}

#[cfg(test)]
impl ExchangeRate {
    pub fn test_fresh_rate() -> Self {
        Self {
            multiplier: 111439,
            decimals: 28,
            timestamp: env::block_timestamp(),
            recency_duration: env::block_timestamp() + 1000000000,
        }
    }

    pub fn test_old_rate() -> Self {
        Self {
            multiplier: 111439,
            decimals: 28,
            timestamp: env::block_timestamp(),
            recency_duration: env::block_timestamp(),
        }
    }

    pub fn test_create_rate(multiplier: u128, decimals: u8) -> Self {
        Self {
            multiplier: multiplier,
            decimals: decimals,
            timestamp: env::block_timestamp(),
            recency_duration: env::block_timestamp(),
        }
    }
}

#[cfg(test)]
impl ExchangeRates {
    pub fn test_fresh_rate() -> Self {
        Self {
            current: ExchangeRate::test_fresh_rate(),
            smooth: ExchangeRate::test_fresh_rate(),
        }
    }

    pub fn test_old_rate() -> Self {
        Self {
            current: ExchangeRate::test_old_rate(),
            smooth: ExchangeRate::test_old_rate(),
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::{ExchangeRate, ExchangeRates};

    #[test]
    fn test_exchange_rate() {
        let first = ExchangeRate::test_create_rate(520944008, 32);
        let second = ExchangeRate::test_create_rate(52296, 28);
        assert!(first < second);

        let first = ExchangeRate::test_create_rate(522960000, 32);
        let second = ExchangeRate::test_create_rate(52296, 28);
        assert!(first == second);

        let first = ExchangeRate::test_create_rate(529944008, 32);
        let second = ExchangeRate::test_create_rate(52296, 28);
        assert!(first > second);

        let first = ExchangeRate::test_create_rate(529944008, 32);
        let second = ExchangeRate::test_create_rate(52994, 28);
        assert_eq!(first.to_decimals(28), second);

        let first = ExchangeRate::test_create_rate(529940000, 32);
        let second = ExchangeRate::test_create_rate(52994, 28);
        assert_eq!(first, second.to_decimals(28));

        let first = ExchangeRate::test_create_rate(52994, 28);
        let second = ExchangeRate::test_create_rate(52994, 28);
        assert_eq!(first, second.to_decimals(28));

        let first = ExchangeRate::test_create_rate(52994, 28);
        let second = ExchangeRate::test_create_rate(62994, 28);
        assert_ne!(first, second.to_decimals(28));
    }

    #[test]
    fn test_exchange_rates() {
        let first = ExchangeRate::test_create_rate(520944008, 32);
        let second = ExchangeRate::test_create_rate(52296, 28);

        let rates = ExchangeRates {
            current: second,
            smooth: first,
        };
        assert!(rates.max() == second);
        assert!(rates.min() == first);

        let rates = ExchangeRates {
            current: first,
            smooth: second,
        };
        assert!(rates.max() == second);
        assert!(rates.min() == first);

        let first = second;
        let rates = ExchangeRates {
            current: first,
            smooth: second,
        };
        assert!(rates.max() == second);
        assert!(rates.min() == first);
    }
}
