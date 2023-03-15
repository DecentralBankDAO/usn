use super::*;
use crate::*;

pub const MIN_BOOSTER_MULTIPLIER: u32 = 10000;

const CONFIG_DURATION_SEC: DurationSec = if cfg!(feature = "mainnet") || cfg!(feature = "testnet") {
    90
} else {
    3600
};

/// Burrow config
#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Config {
    /// The account ID of the booster token contract.
    pub booster_token_id: TokenId,

    /// The number of decimals of the booster fungible token.
    pub booster_decimals: u8,

    /// The total number of different assets
    pub max_num_assets: u32,

    /// The maximum number of seconds expected from the oracle price call.
    pub maximum_recency_duration_sec: DurationSec,

    /// Maximum staleness duration of the price data timestamp.
    /// Because NEAR protocol doesn't implement the gas auction right now, the only reason to
    /// delay the price updates are due to the shard congestion.
    /// This parameter can be updated in the future by the owner.
    pub maximum_staleness_duration_sec: DurationSec,

    /// The minimum duration to stake booster token in seconds.
    pub minimum_staking_duration_sec: DurationSec,

    /// The maximum duration to stake booster token in seconds.
    pub maximum_staking_duration_sec: DurationSec,

    /// The rate of xBooster for the amount of Booster given for the maximum staking duration.
    /// Assuming the 100% multiplier at the minimum staking duration. Should be no less than 100%.
    /// E.g. 20000 means 200% multiplier (or 2X).
    pub x_booster_multiplier_at_maximum_staking_duration: u32,

    /// Whether an account with bad debt can be liquidated using reserves.
    /// The account should have borrowed sum larger than the collateral sum.
    pub force_closing_enabled: bool,
}

impl Config {
    pub fn assert_valid(&self) {
        assert!(
            self.minimum_staking_duration_sec < self.maximum_staking_duration_sec,
            "The maximum staking duration must be greater than minimum staking duration"
        );
        assert!(
            self.x_booster_multiplier_at_maximum_staking_duration >= MIN_BOOSTER_MULTIPLIER,
            "xBooster multiplier should be no less than 100%"
        );
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            booster_token_id: env::current_account_id(),
            booster_decimals: USN_DECIMALS,
            max_num_assets: 20,
            maximum_recency_duration_sec: CONFIG_DURATION_SEC,
            maximum_staleness_duration_sec: 15,
            minimum_staking_duration_sec: 2592000,
            maximum_staking_duration_sec: 31104000,
            x_booster_multiplier_at_maximum_staking_duration: 120000,
            force_closing_enabled: true,
        }
    }
}

impl Burrow {
    pub fn internal_config(&self) -> Config {
        self.config.get().unwrap()
    }

    pub fn update_config(&mut self, config: Config) {
        config.assert_valid();
        self.config.set(&config);
    }

    pub fn add_asset(&mut self, token_id: AccountId, asset_config: AssetConfig) {
        asset_config.assert_valid();
        assert!(self.asset_ids.insert(&token_id));
        self.internal_set_asset(&token_id, Asset::new(env::block_timestamp(), asset_config))
    }

    pub fn update_asset(&mut self, token_id: AccountId, asset_config: AssetConfig) {
        asset_config.assert_valid();
        let mut asset = self.internal_unwrap_asset(&token_id);
        if asset.config.extra_decimals != asset_config.extra_decimals {
            assert!(
                asset.borrowed.balance == 0 && asset.supplied.balance == 0 && asset.reserved == 0,
                "Can't change extra decimals if any of the balances are not 0"
            );
        }
        asset.config = asset_config;
        self.internal_set_asset(&token_id, asset);
    }

    pub fn add_asset_farm_reward(
        &mut self,
        farm_id: FarmId,
        reward_token_id: AccountId,
        new_reward_per_day: U128,
        new_booster_log_base: U128,
        reward_amount: U128,
    ) {
        match &farm_id {
            FarmId::Supplied(token_id) | FarmId::Borrowed(token_id) => {
                assert!(self.assets.contains_key(token_id));
            }
            FarmId::NetTvl => {}
        };
        let reward_token_id: TokenId = reward_token_id.into();
        let mut reward_asset = self.internal_unwrap_asset(&reward_token_id);
        assert!(
            reward_asset.reserved >= reward_amount.0
                && reward_asset.available_amount() >= reward_amount.0,
            "Not enough reserved reward balance"
        );
        reward_asset.reserved -= reward_amount.0;
        self.internal_set_asset(&reward_token_id, reward_asset);
        let mut asset_farm = self
            .internal_get_asset_farm(&farm_id, false)
            .unwrap_or_else(|| AssetFarm {
                block_timestamp: env::block_timestamp(),
                rewards: HashMap::new(),
                inactive_rewards: LookupMap::new(StorageKey::InactiveAssetFarmRewards {
                    farm_id: farm_id.clone(),
                }),
            });

        let mut asset_farm_reward = asset_farm
            .rewards
            .remove(&reward_token_id)
            .or_else(|| asset_farm.internal_remove_inactive_asset_farm_reward(&reward_token_id))
            .unwrap_or_default();
        asset_farm_reward.reward_per_day = new_reward_per_day.into();
        asset_farm_reward.booster_log_base = new_booster_log_base.into();
        asset_farm_reward.remaining_rewards += reward_amount.0;
        asset_farm
            .rewards
            .insert(reward_token_id, asset_farm_reward);
        self.internal_set_asset_farm(&farm_id, asset_farm);
    }
}
