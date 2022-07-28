#![deny(warnings)]
mod event;
mod ft;
mod owner;
mod stable;
mod storage;
mod treasury;

use near_contract_standards::fungible_token::core::FungibleTokenCore;
use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC,
};
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_contract_standards::fungible_token::resolver::FungibleTokenResolver;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, LookupMap, UnorderedMap, UnorderedSet};
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    assert_one_yocto, env, ext_contract, is_promise_success, near_bindgen, sys, AccountId, Balance,
    BorshStorageKey, Gas, PanicOnDefault, Promise, PromiseOrValue, ONE_YOCTO,
};

use std::fmt::Debug;

use crate::ft::FungibleTokenFreeStorage;
use stable::{usdt_id, AssetInfo, StableTreasury};

uint::construct_uint!(
    pub struct U256(4);
);

const NO_DEPOSIT: Balance = 0;
const USN_DECIMALS: u8 = 18;
const GAS_FOR_REFUND_PROMISE: Gas = Gas(5_000_000_000_000);
const GAS_FOR_FT_TRANSFER: Gas = Gas(25_000_000_000_000);

#[derive(BorshStorageKey, BorshSerialize)]
enum StorageKey {
    Guardians,
    Token,
    TokenMetadata,
    Blacklist,
    _TreasuryData,
    StableTreasury,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub enum BlackListStatus {
    // An address might be using
    Allowable,
    // All acts with an address have to be banned
    Banned,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub enum ContractStatus {
    Working,
    Paused,
}

impl std::fmt::Display for ContractStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContractStatus::Working => write!(f, "working"),
            ContractStatus::Paused => write!(f, "paused"),
        }
    }
}

/// USN v1 accumulated commission.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Default)]
#[serde(crate = "near_sdk::serde")]
pub struct CommissionV1 {
    usn: Balance,
    near: Balance,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct CommissionOutput {
    usn: U128,
    near: U128,
}

impl From<CommissionV1> for CommissionOutput {
    fn from(commission: CommissionV1) -> Self {
        Self {
            usn: U128::from(commission.usn),
            near: U128::from(commission.near),
        }
    }
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    owner_id: AccountId,
    proposed_owner_id: AccountId,
    guardians: UnorderedSet<AccountId>,
    token: FungibleTokenFreeStorage,
    metadata: LazyOption<FungibleTokenMetadata>,
    black_list: LookupMap<AccountId, BlackListStatus>,
    status: ContractStatus,
    commission: CommissionV1,
    stable_treasury: StableTreasury,
}

const DATA_IMAGE_SVG_NEAR_ICON: &str =
    "data:image/svg+xml;charset=UTF-8,%3Csvg width='38' height='38' viewBox='0 0 38 38' fill='none' xmlns='http://www.w3.org/2000/svg'%3E%3Crect width='38' height='38' rx='19' fill='black'/%3E%3Cpath d='M14.8388 10.6601C14.4203 10.1008 13.6748 9.86519 12.9933 10.0768C12.3119 10.2885 11.85 10.8991 11.85 11.5883V14.7648H8V17.9412H11.85V20.0589H8V23.2353H11.85V28H15.15V16.5108L23.1612 27.2165C23.5797 27.7758 24.3252 28.0114 25.0067 27.7997C25.6881 27.5881 26.15 26.9775 26.15 26.2882V23.2353H30V20.0589H26.15V17.9412H30V14.7648H26.15V10.0001H22.85V21.3658L14.8388 10.6601Z' fill='white'/%3E%3C/svg%3E";

#[ext_contract(ext_ft_api)]
pub trait FtApi {
    fn ft_transfer(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
    ) -> PromiseOrValue<U128>;
}

#[ext_contract(ext_self)]
trait ContractCallback {
    #[private]
    fn handle_withdraw_refund(&mut self, account_id: AccountId, token_id: AccountId, amount: U128);
}

trait ContractCallback {
    fn handle_withdraw_refund(&mut self, account_id: AccountId, token_id: AccountId, amount: U128);
}

#[near_bindgen]
impl ContractCallback for Contract {
    #[private]
    fn handle_withdraw_refund(&mut self, account_id: AccountId, token_id: AccountId, amount: U128) {
        if !is_promise_success() {
            self.stable_treasury
                .refund(&mut self.token, &account_id, &token_id, amount.into());
            env::log_str(&format!(
                "Refund ${} of USN to {} after {} error",
                amount.0, account_id, token_id,
            ));
        }
    }
}

#[near_bindgen]
impl Contract {
    /// Initializes the contract owned by the given `owner_id` with default metadata.
    #[init]
    pub fn new(owner_id: AccountId) -> Self {
        let metadata = FungibleTokenMetadata {
            spec: FT_METADATA_SPEC.to_string(),
            name: "USN".to_string(),
            symbol: "USN".to_string(),
            icon: Some(DATA_IMAGE_SVG_NEAR_ICON.to_string()),
            reference: None,
            reference_hash: None,
            decimals: USN_DECIMALS,
        };

        let this = Self {
            owner_id: owner_id.clone(),
            proposed_owner_id: owner_id,
            guardians: UnorderedSet::new(StorageKey::Guardians),
            token: FungibleTokenFreeStorage::new(StorageKey::Token),
            metadata: LazyOption::new(StorageKey::TokenMetadata, Some(&metadata)),
            black_list: LookupMap::new(StorageKey::Blacklist),
            status: ContractStatus::Working,
            commission: CommissionV1::default(),
            stable_treasury: StableTreasury::new(StorageKey::StableTreasury),
        };

        this
    }

    pub fn upgrade_name_symbol(&mut self, name: String, symbol: String) {
        self.assert_owner();
        let mut metadata = self.metadata.take().unwrap();
        metadata.name = name;
        metadata.symbol = symbol;
        self.metadata.replace(&metadata);
    }

    pub fn upgrade_icon(&mut self, data: String) {
        self.assert_owner();
        let mut metadata = self.metadata.take().unwrap();
        metadata.icon = Some(data);
        self.metadata.replace(&metadata);
    }

    pub fn blacklist_status(&self, account_id: &AccountId) -> BlackListStatus {
        return match self.black_list.get(account_id) {
            Some(x) => x.clone(),
            None => BlackListStatus::Allowable,
        };
    }

    pub fn add_to_blacklist(&mut self, account_id: &AccountId) {
        self.assert_owner();
        self.black_list.insert(account_id, &BlackListStatus::Banned);
    }

    pub fn remove_from_blacklist(&mut self, account_id: &AccountId) {
        self.assert_owner();
        self.black_list.remove(account_id);
    }

    pub fn destroy_black_funds(&mut self, account_id: &AccountId) {
        self.assert_owner();
        assert_eq!(self.blacklist_status(&account_id), BlackListStatus::Banned);
        let black_balance = self.ft_balance_of(account_id.clone());
        if black_balance.0 <= 0 {
            env::panic_str("The account doesn't have enough balance");
        }
        self.token.accounts.insert(account_id, &0u128);
        self.token.total_supply = self
            .token
            .total_supply
            .checked_sub(u128::from(black_balance))
            .expect("Failed to decrease total supply");
    }

    /// Pauses the contract. Only can be called by owner or guardians.
    #[payable]
    pub fn pause(&mut self) {
        assert_one_yocto();
        self.assert_owner_or_guardian();
        self.status = ContractStatus::Paused;
    }

    /// Resumes the contract. Only can be called by owner.
    pub fn resume(&mut self) {
        self.assert_owner();
        self.status = ContractStatus::Working;
    }

    pub fn contract_status(&self) -> ContractStatus {
        self.status.clone()
    }

    /// Returns the name of the token.
    pub fn name(&self) -> String {
        self.metadata.get().unwrap().name
    }

    /// Returns the symbol of the token.
    pub fn symbol(&self) -> String {
        self.metadata.get().unwrap().symbol
    }

    /// Returns the decimals places of the token.
    pub fn decimals(&self) -> u8 {
        self.metadata.get().unwrap().decimals
    }

    pub fn version(&self) -> String {
        format!("{}:{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
    }

    pub fn commission(&self) -> CommissionOutput {
        self.commission.clone().into()
    }

    /// This is NOOP implementation. KEEP IT if you haven't changed contract state.
    /// Should only be called by this contract on migration.
    /// This method is called from `upgrade()` method.
    /// For next version upgrades, change this function.
    #[init(ignore_state)]
    #[private]
    pub fn migrate() -> Self {
        use near_sdk::Timestamp;

        #[derive(BorshSerialize, BorshDeserialize)]
        struct ExchangeRate {
            multiplier: u128,
            decimals: u8,
            timestamp: Timestamp,
            recency_duration: Timestamp,
        }
        #[derive(BorshSerialize, BorshDeserialize)]
        struct ExchangeRates {
            pub current: ExchangeRate,
            pub smooth: ExchangeRate,
        }

        #[derive(BorshSerialize, BorshDeserialize)]
        struct Oracle {
            pub last_report: Option<ExchangeRates>,
        }

        #[derive(BorshDeserialize, BorshSerialize)]
        struct ExponentialSpreadParams {
            pub min: f64,
            pub max: f64,
            pub scaler: f64,
        }

        #[derive(BorshDeserialize, BorshSerialize)]
        enum Spread {
            Fixed(Balance),
            Exponential(ExponentialSpreadParams),
        }

        #[derive(BorshDeserialize, BorshSerialize)]
        struct CacheItem {
            pub timestamp: Timestamp,
            pub value: f64,
            pub smoothed_value: f64,
            pub n: u8,
        }

        #[derive(BorshDeserialize, BorshSerialize)]
        struct IntervalCache {
            pub items: Vec<CacheItem>,
        }

        #[derive(BorshDeserialize, BorshSerialize)]
        struct TreasuryData {
            pub cache: IntervalCache,
        }

        #[derive(BorshDeserialize, BorshSerialize)]
        struct ExchangeRateValue {
            multiplier: U128,
            decimals: u8,
        }

        #[derive(BorshDeserialize, BorshSerialize)]
        struct MinMaxRate {
            max_previous: Option<ExchangeRateValue>,
            max_current: Option<ExchangeRateValue>,
            min_previous: Option<ExchangeRateValue>,
            min_current: Option<ExchangeRateValue>,
            timestamp: Timestamp,
        }

        #[derive(BorshDeserialize, BorshSerialize)]
        struct VolumeCacheItem {
            usn: U128,
            near: U128,
            timestamp: Timestamp,
        }

        #[derive(BorshDeserialize, BorshSerialize)]
        struct VolumeCache {
            time_slot: Timestamp,
            max_age: Timestamp,
            sum_usn: U128,
            sum_near: U128,
            items: Vec<VolumeCacheItem>,
        }

        #[derive(BorshDeserialize, BorshSerialize)]
        struct VolumeHistory {
            pub one_hour: VolumeCache,
            pub five_min: VolumeCache,
        }

        #[derive(BorshDeserialize, BorshSerialize)]
        pub struct OldStableTreasury {
            stable_token: UnorderedMap<AccountId, AssetInfo>,
        }

        #[derive(BorshDeserialize, BorshSerialize)]
        struct PrevContract {
            owner_id: AccountId,
            proposed_owner_id: AccountId,
            guardians: UnorderedSet<AccountId>,
            token: FungibleTokenFreeStorage,
            metadata: LazyOption<FungibleTokenMetadata>,
            black_list: LookupMap<AccountId, BlackListStatus>,
            status: ContractStatus,
            oracle: Oracle,
            spread: Spread,
            commission: CommissionV1,
            treasury: LazyOption<TreasuryData>,
            usn2near: VolumeHistory,
            near2usn: VolumeHistory,
            best_rate: MinMaxRate,
            stable_treasury: OldStableTreasury,
        }

        let prev: PrevContract = env::state_read().expect("Contract is not initialized");

        Self {
            owner_id: prev.owner_id,
            proposed_owner_id: prev.proposed_owner_id,
            guardians: prev.guardians,
            token: prev.token,
            metadata: prev.metadata,
            black_list: prev.black_list,
            status: prev.status,
            commission: prev.commission,
            stable_treasury: prev.stable_treasury.stable_token.into(),
        }
    }

    fn abort_if_pause(&self) {
        if self.status == ContractStatus::Paused {
            env::panic_str("The contract is under maintenance")
        }
    }

    fn abort_if_blacklisted(&self, account_id: &AccountId) {
        if self.blacklist_status(account_id) != BlackListStatus::Allowable {
            env::panic_str(&format!("Account '{}' is banned", account_id));
        }
    }

    fn on_tokens_burned(&mut self, account_id: AccountId, amount: Balance) {
        event::emit::ft_burn(&account_id, amount, None)
    }
}

#[no_mangle]
pub fn upgrade() {
    env::setup_panic_hook();

    let contract: Contract = env::state_read().expect("Contract is not initialized");
    contract.assert_owner();

    const MIGRATE_METHOD_NAME: &[u8; 7] = b"migrate";
    const UPDATE_GAS_LEFTOVER: Gas = Gas(5_000_000_000_000);

    unsafe {
        // Load code into register 0 result from the input argument if factory call or from promise if callback.
        sys::input(0);
        // Create a promise batch to update current contract with code from register 0.
        let promise_id = sys::promise_batch_create(
            env::current_account_id().as_bytes().len() as u64,
            env::current_account_id().as_bytes().as_ptr() as u64,
        );
        // Deploy the contract code from register 0.
        sys::promise_batch_action_deploy_contract(promise_id, u64::MAX, 0);
        // Call promise to migrate the state.
        // Batched together to fail upgrade if migration fails.
        sys::promise_batch_action_function_call(
            promise_id,
            MIGRATE_METHOD_NAME.len() as u64,
            MIGRATE_METHOD_NAME.as_ptr() as u64,
            0,
            0,
            0,
            (env::prepaid_gas() - env::used_gas() - UPDATE_GAS_LEFTOVER).0,
        );
        sys::promise_return(promise_id);
    }
}

/// The core methods for a basic fungible token. Extension standards may be
/// added in addition to this macro.

#[near_bindgen]
impl FungibleTokenCore for Contract {
    #[payable]
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        self.abort_if_pause();
        self.abort_if_blacklisted(&env::predecessor_account_id());
        self.token.ft_transfer(receiver_id, amount, memo);
    }

    #[payable]
    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        self.abort_if_pause();
        self.abort_if_blacklisted(&env::predecessor_account_id());
        self.token
            .ft_transfer_call(receiver_id.clone(), amount, memo, msg)
    }

    fn ft_total_supply(&self) -> U128 {
        self.token.ft_total_supply()
    }

    fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        self.token.ft_balance_of(account_id)
    }
}

#[near_bindgen]
impl FungibleTokenResolver for Contract {
    #[private]
    fn ft_resolve_transfer(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> U128 {
        let sender_id: AccountId = sender_id.into();
        let (used_amount, burned_amount) =
            self.token
                .internal_ft_resolve_transfer(&sender_id, receiver_id, amount);
        if burned_amount > 0 {
            self.on_tokens_burned(sender_id, burned_amount);
        }
        used_amount.into()
    }
}

#[near_bindgen]
impl FungibleTokenMetadataProvider for Contract {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.get().unwrap()
    }
}

#[near_bindgen]
impl FungibleTokenReceiver for Contract {
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        self.abort_if_pause();
        self.abort_if_blacklisted(&sender_id);

        // Empty message is used for stable coin depositing.
        assert!(msg.is_empty());

        let token_id = env::predecessor_account_id();

        self.stable_treasury
            .deposit(&mut self.token, &sender_id, &token_id, amount.into());

        // Unused tokens: 0.
        PromiseOrValue::Value(U128(0))
    }
}

#[near_bindgen]
impl Contract {
    #[payable]
    pub fn withdraw(&mut self, asset_id: Option<AccountId>, amount: U128) -> Promise {
        let account_id = env::predecessor_account_id();
        let asset_id = asset_id.unwrap_or(usdt_id());

        assert_one_yocto();
        self.abort_if_pause();
        self.abort_if_blacklisted(&account_id);

        let asset_amount =
            self.stable_treasury
                .withdraw(&mut self.token, &account_id, &asset_id, amount.into());

        ext_ft_api::ft_transfer(
            account_id.clone(),
            asset_amount.into(),
            None,
            asset_id.clone(),
            ONE_YOCTO,
            GAS_FOR_FT_TRANSFER,
        )
        .as_return()
        .then(ext_self::handle_withdraw_refund(
            account_id,
            asset_id,
            amount,
            env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_REFUND_PROMISE,
        ))
    }

    pub fn add_stable_asset(&mut self, asset_id: &AccountId, decimals: u8) {
        self.assert_owner();
        self.stable_treasury.add_asset(asset_id, decimals);
    }

    pub fn enable_stable_asset(&mut self, asset_id: &AccountId) {
        self.assert_owner();
        self.stable_treasury.enable_asset(asset_id);
    }

    pub fn disable_stable_asset(&mut self, asset_id: &AccountId) {
        self.assert_owner();
        self.stable_treasury.disable_asset(asset_id);
    }

    pub fn treasury(&self) -> Vec<(AccountId, AssetInfo)> {
        self.stable_treasury.supported_assets()
    }

    pub fn set_commission_rate(&mut self, rate: u32) {
        self.assert_owner();
        self.stable_treasury.set_commission_rate(rate);
    }

    pub fn commission_rate(&self) -> u32 {
        self.stable_treasury.commission_rate()
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, Balance, ONE_YOCTO};

    use super::*;

    fn get_context(predecessor_account_id: AccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id(accounts(0))
            .signer_account_id(predecessor_account_id.clone())
            .predecessor_account_id(predecessor_account_id);
        builder
    }

    #[test]
    fn test_new() {
        const TOTAL_SUPPLY: Balance = 0;
        let mut context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = Contract::new(accounts(1));
        testing_env!(context.is_view(true).build());
        assert_eq!(contract.ft_total_supply().0, TOTAL_SUPPLY);
        assert_eq!(contract.ft_balance_of(accounts(1)).0, TOTAL_SUPPLY);
    }

    #[test]
    #[should_panic(expected = "The contract is not initialized")]
    fn test_default() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let _contract = Contract::default();
    }

    #[test]
    fn test_ownership() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1));
        contract.propose_new_owner(accounts(2));
        assert_eq!(contract.owner_id, accounts(1));
        testing_env!(context.predecessor_account_id(accounts(2)).build());
        contract.accept_ownership();
        assert_eq!(contract.owner_id, accounts(2));
    }

    #[test]
    fn test_transfer() {
        const AMOUNT: Balance = 3_000_000_000_000_000_000_000_000;

        let mut context = get_context(accounts(2));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(2));
        contract.token.internal_deposit(&accounts(2), AMOUNT);

        testing_env!(context
            .storage_usage(env::storage_usage())
            .predecessor_account_id(accounts(1))
            .build());
        // Paying for account registration, aka storage deposit

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(ONE_YOCTO)
            .predecessor_account_id(accounts(2))
            .build());
        let transfer_amount = AMOUNT / 3;
        contract.ft_transfer(accounts(1), transfer_amount.into(), None);

        testing_env!(context
            .storage_usage(env::storage_usage())
            .account_balance(env::account_balance())
            .is_view(true)
            .attached_deposit(0)
            .build());
        assert_eq!(
            contract.ft_balance_of(accounts(2)).0,
            (AMOUNT - transfer_amount)
        );
        assert_eq!(contract.ft_balance_of(accounts(1)).0, transfer_amount);
    }

    #[test]
    fn test_blacklist() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1));

        // Act as a user.
        testing_env!(context.predecessor_account_id(accounts(2)).build());

        assert_eq!(
            contract.blacklist_status(&accounts(1)),
            BlackListStatus::Allowable
        );

        contract.token.internal_deposit(&accounts(2), 1000);
        assert_eq!(contract.ft_balance_of(accounts(2)), U128::from(1000));

        // Act as owner.
        testing_env!(context.predecessor_account_id(accounts(1)).build());

        contract.add_to_blacklist(&accounts(2));
        assert_eq!(
            contract.blacklist_status(&accounts(2)),
            BlackListStatus::Banned
        );

        contract.remove_from_blacklist(&accounts(2));
        assert_eq!(
            contract.blacklist_status(&accounts(2)),
            BlackListStatus::Allowable
        );

        contract.add_to_blacklist(&accounts(2));
        let total_supply_before = contract.token.total_supply;

        assert_ne!(contract.ft_balance_of(accounts(2)), U128::from(0));

        contract.destroy_black_funds(&accounts(2));
        assert_ne!(total_supply_before, contract.token.total_supply);

        assert_eq!(contract.ft_balance_of(accounts(2)), U128::from(0));

        assert_eq!(
            contract.blacklist_status(&accounts(2)),
            BlackListStatus::Banned
        );
    }

    #[test]
    #[should_panic]
    fn test_user_cannot_destroy_black_funds() {
        let mut context = get_context(accounts(2));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(2));
        testing_env!(context
            .storage_usage(env::storage_usage())
            .predecessor_account_id(accounts(1))
            .build());

        contract.add_to_blacklist(&accounts(1));
    }

    #[test]
    fn test_maintenance() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1));
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(ONE_YOCTO)
            .predecessor_account_id(accounts(1))
            .current_account_id(accounts(1))
            .signer_account_id(accounts(1))
            .build());
        assert_eq!(contract.contract_status(), ContractStatus::Working);
        contract.pause();
        assert_eq!(contract.contract_status(), ContractStatus::Paused);
        contract.resume();
        assert_eq!(contract.contract_status(), ContractStatus::Working);
        contract.pause();
        contract.ft_total_supply();
    }

    #[test]
    #[should_panic]
    fn test_extend_guardians_by_user() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1));
        testing_env!(context.predecessor_account_id(accounts(2)).build());
        contract.extend_guardians(vec![accounts(3)]);
    }

    #[test]
    fn test_guardians() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1));
        testing_env!(context.predecessor_account_id(accounts(1)).build());
        contract.extend_guardians(vec![accounts(2)]);
        assert!(contract.guardians.contains(&accounts(2)));
        contract.remove_guardians(vec![accounts(2)]);
        assert!(!contract.guardians.contains(&accounts(2)));
    }

    #[test]
    fn test_view_guardians() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1));
        testing_env!(context.predecessor_account_id(accounts(1)).build());
        contract.extend_guardians(vec![accounts(2)]);
        assert_eq!(contract.guardians()[0], accounts(2));
        contract.remove_guardians(vec![accounts(2)]);
        assert_eq!(contract.guardians().len(), 0);
    }

    #[test]
    #[should_panic]
    fn test_cannot_remove_guardians() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1));
        testing_env!(context.predecessor_account_id(accounts(1)).build());
        contract.extend_guardians(vec![accounts(2)]);
        assert!(contract.guardians.contains(&accounts(2)));
        contract.remove_guardians(vec![accounts(3)]);
    }

    #[test]
    fn test_deposit_auto_registration() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());

        let mut contract = Contract::new(accounts(1));

        testing_env!(context.predecessor_account_id(usdt_id()).build());
        contract.ft_on_transfer(accounts(2), U128(1000000000), "".to_string());

        testing_env!(context
            .predecessor_account_id(accounts(2))
            .attached_deposit(ONE_YOCTO)
            .build());

        contract.withdraw(None, U128(999900000000000000000));
    }
}
