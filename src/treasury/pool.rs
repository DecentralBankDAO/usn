use crate::*;

pub(super) const USDT_DECIMALS: u8 = 6;

struct PoolsConfig {
    pub ref_address: &'static str,
    pub pools: &'static [(u64, &'static [&'static (&'static str, u8)])],
}

const CONFIG: PoolsConfig = if cfg!(feature = "mainnet") {
    PoolsConfig {
        ref_address: "v2.ref-finance.near",
        pools: &[(
            3020,
            &[
                &("usn", USN_DECIMALS),
                &(
                    "dac17f958d2ee523a2206206994597c13d831ec7.factory.bridge.near",
                    USDT_DECIMALS,
                ),
            ],
        )],
    }
} else if cfg!(feature = "testnet") {
    PoolsConfig {
        ref_address: "ref-finance-101.testnet",
        pools: &[(
            356,
            &[
                &("usdn.testnet", USN_DECIMALS),
                &("usdt.fakes.testnet", USDT_DECIMALS),
            ],
        )],
    }
} else {
    PoolsConfig {
        ref_address: "ref.test.near",
        pools: &[
            (
                0,
                &[
                    &("usn.test.near", USN_DECIMALS),
                    &("usdt.test.near", USDT_DECIMALS),
                ],
            ),
            (
                1,
                &[
                    &("usn.test.near", USN_DECIMALS),
                    &("usdt.test.near", USDT_DECIMALS),
                ],
            ),
        ],
    }
};

#[near_bindgen]
impl Contract {
    pub fn pools(&self) -> Vec<u64> {
        CONFIG.pools.iter().map(|&(pool_id, _)| pool_id).collect()
    }
}

pub struct Pool {
    pub ref_id: AccountId,
    pub id: u64,
    pub tokens: Vec<AccountId>,
    pub decimals: Vec<u8>,
}

impl Pool {
    pub fn from_config_with_assert(pool_id: u64) -> Self {
        CONFIG
            .pools
            .iter()
            .find_map(|&(id, tokens)| {
                if pool_id == id {
                    Some(Self {
                        ref_id: CONFIG.ref_address.parse().unwrap(),
                        id: pool_id,
                        tokens: tokens.iter().map(|t| t.0.parse().unwrap()).collect(),
                        decimals: tokens.iter().map(|t| t.1).collect(),
                    })
                } else {
                    None
                }
            })
            .unwrap_or_else(|| env::panic_str(&format!("pool_id {} is not allowed", pool_id)))
    }

    /// Extends the whole part of the amount (the left part to the decimal point)
    /// into the token amounts considering decimal precision of each token.
    pub fn extend_decimals(
        &self,
        whole_amount: u128,
    ) -> impl Iterator<Item = (&AccountId, u128)> + '_ + Clone {
        self.tokens.iter().zip(
            self.decimals
                .iter()
                .map(move |decimals| extend_decimals(whole_amount, *decimals)),
        )
    }

    pub fn pool() -> Self {
        let pool_config = CONFIG.pools.first().unwrap();
        Self {
            ref_id: CONFIG.ref_address.parse().unwrap(),
            id: pool_config.0,
            tokens: pool_config.1.iter().map(|t| t.0.parse().unwrap()).collect(),
            decimals: pool_config.1.iter().map(|t| t.1).collect(),
        }
    }
}

pub fn extend_decimals(whole: u128, decimals: u8) -> u128 {
    whole * 10u128.pow(decimals as u32)
}

pub fn remove_decimals(amount: u128, decimals: u8) -> u128 {
    amount / 10u128.pow(decimals as u32)
}
