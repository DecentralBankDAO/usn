mod account;
mod account_asset;
mod account_farm;
mod account_view;
mod actions;
mod asset;
mod asset_config;
mod asset_farm;
mod asset_view;
mod big_decimal;
mod booster_staking;
mod common;
mod config;
mod events;
mod fungible_token;
mod pool;
mod price_receiver;
mod prices;
mod storage;
mod storage_tracker;
mod utils;

use account::*;
use account_asset::*;
pub use account_farm::FarmId;
use account_farm::*;
use account_view::*;
use actions::*;
use asset::*;
use asset_config::*;
use asset_farm::*;
use asset_view::*;
use big_decimal::*;
use booster_staking::*;
use common::*;
pub use common::{u128_dec_format, u64_dec_format};
pub use config::Config;
use config::*;
use pool::*;
use prices::*;
use storage::*;
use storage_tracker::*;
use utils::TokenId;
use utils::*;

use crate::oracle::{Price, CONFIG};
use crate::*;
use near_sdk::collections::UnorderedMap;
pub use near_sdk::{ext_contract, Duration, Timestamp};
pub use near_sdk::{log, serde_json::json};
pub use once_cell::sync::Lazy;
pub use std::collections::{HashMap, HashSet};
pub use std::sync::Mutex;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Burrow {
    pub accounts: UnorderedMap<AccountId, VAccount>,
    pub storage: LookupMap<AccountId, VStorage>,
    pub assets: LookupMap<TokenId, VAsset>,
    pub asset_farms: LookupMap<FarmId, VAssetFarm>,
    pub asset_ids: UnorderedSet<TokenId>,
    pub config: LazyOption<Config>,
    /// The last recorded price info from the oracle. It's used for Net TVL farm computation.
    pub last_prices: HashMap<TokenId, Price>,
}

impl Burrow {
    /// Initializes the contract with the given config. Needs to be called once.
    pub fn new(config: Config) -> Self {
        config.assert_valid();
        Self {
            accounts: UnorderedMap::new(StorageKey::BurrowAccounts),
            storage: LookupMap::new(StorageKey::BurrowStorage),
            assets: LookupMap::new(StorageKey::BurrowAssets),
            asset_farms: LookupMap::new(StorageKey::BurrowAssetFarms),
            asset_ids: UnorderedSet::new(StorageKey::BurrowAssetIds),
            config: LazyOption::new(StorageKey::BurrowConfig, Some(&config)),
            last_prices: HashMap::new(),
        }
    }
}

pub trait OraclePriceReceiver {
    fn oracle_on_call(&mut self, sender_id: AccountId, data: PriceData, msg: String);
}

#[near_bindgen]
impl OraclePriceReceiver for Contract {
    /// The method will execute a given list of actions in the msg using the prices from the `data`
    /// provided by the oracle on behalf of the sender_id.
    /// - Requires to be called by the oracle account ID.
    fn oracle_on_call(&mut self, sender_id: AccountId, data: PriceData, msg: String) {
        assert_eq!(env::predecessor_account_id(), CONFIG.oracle_address());
        let is_liquidator = self.check_guardian_role(&sender_id, GuardianRole::BurrowLiquidator);
        self.burrow
            .oracle_on_call(sender_id, data, msg, &mut self.token, is_liquidator);
    }
}

#[ext_contract(ext_self_burrow)]
trait ExtSelfBurrow {
    fn after_ft_transfer(&mut self, account_id: AccountId, token_id: TokenId, amount: U128)
        -> bool;
}

trait ExtSelfBurrow {
    fn after_ft_transfer(&mut self, account_id: AccountId, token_id: TokenId, amount: U128)
        -> bool;
}

#[near_bindgen]
impl ExtSelfBurrow for Contract {
    #[private]
    fn after_ft_transfer(
        &mut self,
        account_id: AccountId,
        token_id: TokenId,
        amount: U128,
    ) -> bool {
        self.burrow.after_ft_transfer(account_id, token_id, amount)
    }
}

#[near_bindgen]
impl Contract {
    /// Claims all unclaimed farm rewards and starts farming new farms.
    /// If the account_id is given, then it claims farms for the given account_id or uses
    /// predecessor_account_id otherwise.
    #[private]
    #[allow(unused)]
    fn account_farm_claim_all(&mut self, account_id: Option<AccountId>) {
        self.burrow.account_farm_claim_all(account_id);
    }

    /// Returns detailed information about an account for a given account_id.
    /// The information includes all supplied assets, collateral and borrowed.
    /// Each asset includes the current balance and the number of shares.
    pub fn get_account(&self, account_id: AccountId) -> Option<AccountDetailedView> {
        self.burrow.get_account(account_id)
    }

    /// Returns limited account information for accounts from a given index up to a given limit.
    /// The information includes number of shares for collateral and borrowed assets.
    /// This method can be used to iterate on the accounts for liquidation.
    pub fn get_accounts_paged(&self, from_index: Option<u64>, limit: Option<u64>) -> Vec<Account> {
        self.burrow.get_accounts_paged(from_index, limit)
    }

    /// Returns the number of accounts
    pub fn get_num_accounts(&self) -> u32 {
        self.burrow.get_num_accounts()
    }

    /// Executes a given list actions on behalf of the predecessor account.
    /// - Requires one yoctoNEAR.
    #[payable]
    pub fn execute(&mut self, actions: Vec<Action>) {
        assert_one_yocto();
        self.burrow.execute(actions, &mut self.token)
    }

    /// Returns an asset farm for a given farm ID.
    #[private]
    #[allow(unused)]
    fn get_asset_farm(&self, farm_id: FarmId) -> Option<AssetFarm> {
        self.burrow.get_asset_farm(farm_id)
    }

    /// Returns a list of pairs (farm ID, asset farm) for a given list of farm IDs.
    #[private]
    #[allow(unused)]
    fn get_asset_farms(&self, farm_ids: Vec<FarmId>) -> Vec<(FarmId, AssetFarm)> {
        self.burrow.get_asset_farms(farm_ids)
    }

    /// Returns full list of pairs (farm ID, asset farm).
    #[private]
    #[allow(unused)]
    fn get_asset_farms_all(&self) -> Vec<(FarmId, AssetFarm)> {
        self.burrow.get_asset_farms_all()
    }

    /// Returns an asset for a given token_id.
    pub fn get_asset(&self, token_id: AccountId) -> Option<AssetDetailedView> {
        self.burrow.get_asset(token_id)
    }

    /// Returns an list of pairs (token_id, asset) for assets a given list of token_id.
    /// Only returns pais for existing assets.
    pub fn get_assets(&self, token_ids: Vec<AccountId>) -> Vec<AssetDetailedView> {
        self.burrow.get_assets(token_ids)
    }

    /// Returns a list of pairs (token_id, asset) for assets from a given index up to a given limit.
    pub fn get_assets_paged(
        &self,
        from_index: Option<u64>,
        limit: Option<u64>,
    ) -> Vec<(TokenId, Asset)> {
        self.burrow.get_assets_paged(from_index, limit)
    }

    pub fn get_assets_paged_detailed(
        &self,
        from_index: Option<u64>,
        limit: Option<u64>,
    ) -> Vec<AssetDetailedView> {
        self.burrow.get_assets_paged_detailed(from_index, limit)
    }

    /// Stakes a given amount (or all supplied) booster token for a given duration in seconds.
    /// If the previous stake exists, then the new duration should be longer than the previous
    /// remaining staking duration.
    /// Currently is private
    #[private]
    #[payable]
    #[allow(unused)]
    fn account_stake_booster(&mut self, amount: Option<U128>, duration: DurationSec) {
        self.assert_owner();
        assert_one_yocto();
        self.burrow.account_stake_booster(amount, duration)
    }

    #[private]
    #[payable]
    #[allow(unused)]
    fn account_unstake_booster(&mut self) {
        self.assert_owner();
        assert_one_yocto();
        self.burrow.account_unstake_booster()
    }

    /// Returns the current config.
    pub fn get_config(&self) -> Config {
        self.burrow.internal_config()
    }

    /// Updates the current config.
    /// - Requires one yoctoNEAR.
    /// - Requires to be called by the contract owner.
    #[payable]
    pub fn update_config(&mut self, config: Config) {
        assert_one_yocto();
        self.assert_owner();
        self.burrow.update_config(config)
    }

    /// Adds an asset with a given token_id and a given asset_config.
    /// - Panics if the asset config is invalid.
    /// - Panics if an asset with the given token_id already exists.
    /// - Requires one yoctoNEAR.
    /// - Requires to be called by the contract owner.
    #[payable]
    pub fn add_asset(&mut self, token_id: AccountId, asset_config: AssetConfig) {
        assert_one_yocto();
        self.assert_owner();
        self.burrow.add_asset(token_id, asset_config)
    }

    /// Updates the asset config for the asset with the a given token_id.
    /// - Panics if the asset config is invalid.
    /// - Panics if an asset with the given token_id doesn't exist.
    /// - Requires one yoctoNEAR.
    /// - Requires to be called by the contract owner.
    #[payable]
    pub fn update_asset(&mut self, token_id: AccountId, asset_config: AssetConfig) {
        assert_one_yocto();
        self.assert_owner();
        self.burrow.update_asset(token_id, asset_config)
    }

    /// Adds an asset farm reward for the farm with a given farm_id. The reward is of token_id with
    /// the new reward per day amount and a new booster log base. The extra amount of reward is
    /// taken from the asset reserved balance.
    /// - The booster log base should include decimals of the token for better precision of the log
    ///    base. For example, if token decimals is `6` the log base of `10_500_000` will be `10.5`.
    /// - Panics if the farm asset token_id doesn't exists.
    /// - Panics if an asset with the given token_id doesn't exists.
    /// - Panics if an asset with the given token_id doesn't have enough reserved balance.
    /// - Requires one yoctoNEAR.
    /// - Requires to be called by the contract owner.
    #[private]
    #[payable]
    #[allow(unused)]
    fn add_asset_farm_reward(
        &mut self,
        farm_id: FarmId,
        reward_token_id: AccountId,
        new_reward_per_day: U128,
        new_booster_log_base: U128,
        reward_amount: U128,
    ) {
        assert_one_yocto();
        self.assert_owner();
        self.burrow.add_asset_farm_reward(
            farm_id,
            reward_token_id,
            new_reward_per_day,
            new_booster_log_base,
            reward_amount,
        )
    }
}
