#![deny(warnings)]
mod event;
mod ft;
mod history;
mod oracle;
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
use near_sdk::collections::{LazyOption, LookupMap, UnorderedSet};
use near_sdk::json_types::{U128, U64};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    assert_one_yocto, env, ext_contract, is_promise_success, near_bindgen, sys, AccountId, Balance,
    BorshStorageKey, Gas, PanicOnDefault, Promise, PromiseOrValue, ONE_YOCTO,
};

use std::fmt::Debug;

use crate::ft::FungibleTokenFreeStorage;
use history::{MinMaxRate, VolumeHistory};
use oracle::{
    ExchangeRate, ExchangeRateValue, ExchangeRates, Oracle, PriceData, DEFAULT_RATE_DECIMALS,
};
use partial_min_max;
use stable::{usdt_id, StableInfo, StableTreasury};
use treasury::TreasuryData;

uint::construct_uint!(
    pub struct U256(4);
);

const NO_DEPOSIT: Balance = 0;
const USN_DECIMALS: u8 = 18;
const GAS_FOR_REFUND_PROMISE: Gas = Gas(5_000_000_000_000);
const GAS_FOR_BUY_PROMISE: Gas = Gas(10_000_000_000_000);
const GAS_FOR_SELL_PROMISE: Gas = Gas(15_000_000_000_000);
const GAS_FOR_RETURN_VALUE_PROMISE: Gas = Gas(5_000_000_000_000);
const GAS_FOR_FT_TRANSFER: Gas = Gas(25_000_000_000_000);

const MAX_SPREAD: Balance = 50_000; // 0.05 = 5%
const SPREAD_DECIMAL: u8 = 6;
const SPREAD_MAX_SCALER: f64 = 0.4;

#[derive(BorshStorageKey, BorshSerialize)]
enum StorageKey {
    Guardians,
    Token,
    TokenMetadata,
    Blacklist,
    TreasuryData,
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

#[derive(BorshDeserialize, BorshSerialize, Debug, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct HistoryData {
    pub usn2near: VolumeHistory,
    pub near2usn: VolumeHistory,
    pub best_rate: MinMaxRate,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct ExpectedRate {
    pub multiplier: U128,
    pub slippage: U128,
    pub decimals: u8,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Default)]
#[serde(crate = "near_sdk::serde")]
pub struct Commission {
    usn: Balance,
    near: Balance,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct CommissionOutput {
    usn: U128,
    near: U128,
}

impl From<Commission> for CommissionOutput {
    fn from(commission: Commission) -> Self {
        Self {
            usn: U128::from(commission.usn),
            near: U128::from(commission.near),
        }
    }
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct ExchangeResult {
    commission: Commission,
    amount: Balance,
    rate: ExchangeRateValue,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct ExchangeResultOutput {
    commission: CommissionOutput,
    amount: U128,
    rate: ExchangeRateValue,
}

impl From<ExchangeResult> for ExchangeResultOutput {
    fn from(result: ExchangeResult) -> Self {
        Self {
            commission: CommissionOutput::from(result.commission),
            amount: U128::from(result.amount),
            rate: result.rate,
        }
    }
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct ExponentialSpreadParams {
    pub min: f64,
    pub max: f64,
    pub scaler: f64,
}

impl Default for ExponentialSpreadParams {
    fn default() -> Self {
        Self {
            min: 0.001,
            max: 0.005,
            scaler: 0.0000075,
        }
    }
}

#[derive(BorshDeserialize, BorshSerialize)]
pub enum Spread {
    Fixed(Balance),
    Exponential(ExponentialSpreadParams),
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
    oracle: Oracle,
    spread: Spread,
    commission: Commission,
    treasury: LazyOption<TreasuryData>,
    usn2near: VolumeHistory,
    near2usn: VolumeHistory,
    best_rate: MinMaxRate,
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
    fn buy_with_price_callback(
        &mut self,
        account: AccountId,
        near: U128,
        expected: Option<ExpectedRate>,
        #[callback] price: PriceData,
    ) -> U128;

    #[private]
    fn sell_with_price_callback(
        &mut self,
        account: AccountId,
        tokens: U128,
        expected: Option<ExpectedRate>,
        #[callback] price: PriceData,
    ) -> Promise;

    #[private]
    fn handle_refund(&mut self, account: AccountId, attached_deposit: U128);

    #[private]
    fn return_value(&mut self, value: U128) -> U128;

    #[private]
    fn handle_withdraw_refund(
        &mut self,
        account_id: AccountId,
        token_id: AccountId,
        token_amount: U128,
    );
}

trait ContractCallback {
    fn buy_with_price_callback(
        &mut self,
        account: AccountId,
        near: U128,
        expected: Option<ExpectedRate>,
        price: PriceData,
    ) -> U128;

    fn sell_with_price_callback(
        &mut self,
        account: AccountId,
        tokens: U128,
        expected: Option<ExpectedRate>,
        price: PriceData,
    ) -> Promise;

    fn handle_refund(&mut self, account: AccountId, attached_deposit: U128);

    fn return_value(&mut self, value: U128) -> U128;

    fn handle_withdraw_refund(
        &mut self,
        account_id: AccountId,
        token_id: AccountId,
        token_amount: U128,
    );
}

#[near_bindgen]
impl ContractCallback for Contract {
    #[private]
    fn buy_with_price_callback(
        &mut self,
        account: AccountId,
        near: U128,
        expected: Option<ExpectedRate>,
        #[callback] price: PriceData,
    ) -> U128 {
        let rates: ExchangeRates = price.into();

        self.finish_buy(account, near.0, expected, rates).into()
    }

    #[private]
    fn sell_with_price_callback(
        &mut self,
        account: AccountId,
        tokens: U128,
        expected: Option<ExpectedRate>,
        #[callback] price: PriceData,
    ) -> Promise {
        let rates: ExchangeRates = price.into();

        let deposit = self.finish_sell(account.clone(), tokens.0, expected, rates);

        Promise::new(account)
            .transfer(deposit)
            .then(ext_self::return_value(
                deposit.into(),
                env::current_account_id(),
                0,
                GAS_FOR_RETURN_VALUE_PROMISE,
            ))
    }

    #[private]
    fn handle_refund(&mut self, account: AccountId, attached_deposit: U128) {
        if !is_promise_success() {
            Promise::new(account)
                .transfer(attached_deposit.0)
                .as_return();
        }
    }

    #[private]
    fn return_value(&mut self, value: U128) -> U128 {
        assert!(is_promise_success(), "Transfer has failed");
        // TODO: Remember lost value? Unlikely to happen, and only by user error.
        value
    }

    #[private]
    fn handle_withdraw_refund(
        &mut self,
        account_id: AccountId,
        token_id: AccountId,
        token_amount: U128,
    ) {
        if !is_promise_success() {
            self.stable_treasury.deposit(
                &mut self.token,
                &account_id,
                &token_id,
                token_amount.into(),
            );
            env::log_str(&format!(
                "Refund ${} of {} to {}",
                token_amount.0, token_id, account_id
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
            oracle: Oracle::default(),
            spread: Spread::Exponential(ExponentialSpreadParams::default()),
            commission: Commission::default(),
            treasury: LazyOption::new(StorageKey::TreasuryData, Some(&TreasuryData::default())),
            usn2near: VolumeHistory::new(),
            near2usn: VolumeHistory::new(),
            best_rate: MinMaxRate::default(),
            stable_treasury: StableTreasury::new(StorageKey::StableTreasury),
        };

        this
    }

    pub fn upgrade_name_symbol(&mut self, name: String, symbol: String) {
        self.assert_owner();
        let metadata = self.metadata.take();
        if let Some(mut metadata) = metadata {
            metadata.name = name;
            metadata.symbol = symbol;
            self.metadata.replace(&metadata);
        }
    }

    pub fn upgrade_icon(&mut self, data: String) {
        self.assert_owner();
        let metadata = self.metadata.take();
        if let Some(mut metadata) = metadata {
            metadata.icon = Some(data);
            self.metadata.replace(&metadata);
        }
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
        // TODO: Should guardians be able to pause?
        self.assert_owner_or_guardian();
        self.status = ContractStatus::Paused;
    }

    /// Resumes the contract. Only can be called by owner.
    pub fn resume(&mut self) {
        self.assert_owner();
        self.status = ContractStatus::Working;
    }

    fn buy_price(&self, amount_near: u128, rates: &ExchangeRates) -> ExchangeRate {
        let amount_near = U256::from(amount_near);
        let five_min_near = U256::from(self.near2usn.five_min.sum_near());
        let one_hour_near = U256::from(self.near2usn.one_hour.sum_near());
        let one_hour_usn = U256::from(self.near2usn.one_hour.sum_usn());
        let best_prev_rate = self
            .best_rate
            .min_previous()
            .unwrap_or(ExchangeRateValue::from(rates.min()))
            .to_decimals(DEFAULT_RATE_DECIMALS);

        // Pn2u1h =if($VLMn2u1h>0,min(On2ubest(t-5m),$VLMn2u1h/VLMn2u1h), On2ubest(t-5m))
        let price_1hour: ExchangeRateValue = if one_hour_usn > U256::zero() {
            let multiplier = one_hour_usn
                * U256::from(10u128.pow(u32::from(DEFAULT_RATE_DECIMALS - USN_DECIMALS)))
                / one_hour_near;
            let new_price = ExchangeRateValue::new(multiplier.as_u128(), DEFAULT_RATE_DECIMALS);

            partial_min_max::min(new_price, best_prev_rate)
        } else {
            best_prev_rate
        };

        // On2ubest = min (ORACLE_smooth,ORACLE_current)
        // Pn2u(t)=(VLMn2unow+VLMn2u5m)/(VLMn2unow+VLMn2u1h)*Pn2u1h +
        //         (VLMn2u1h—VLMn2u5m)/(VLMn2unow+VLMn2u1h)*On2ubest
        let near2usn_best_rate =
            ExchangeRateValue::from(rates.min()).to_decimals(DEFAULT_RATE_DECIMALS);

        let a = (amount_near + five_min_near) * U256::from(price_1hour.multiplier())
            / (amount_near + one_hour_near);
        let b = (one_hour_near - five_min_near) * U256::from(near2usn_best_rate.multiplier())
            / (amount_near + one_hour_near);

        let multiplier = a + b;
        let price = ExchangeRate::new(multiplier.as_u128(), DEFAULT_RATE_DECIMALS);

        price
    }

    fn sell_price(&self, amount_usn: u128, rates: &ExchangeRates) -> ExchangeRate {
        let amount_usn = U256::from(amount_usn);
        let five_min_usn = U256::from(self.usn2near.five_min.sum_usn());
        let one_hour_usn = U256::from(self.usn2near.one_hour.sum_usn());
        let one_hour_near = U256::from(self.usn2near.one_hour.sum_near());
        let best_prev_rate = self
            .best_rate
            .max_previous()
            .unwrap_or(ExchangeRateValue::from(rates.max()))
            .to_decimals(DEFAULT_RATE_DECIMALS);

        // Pu2n1h = if($VLMu2n1h>0,max(Ou2nbest(t-5m),$VLMu2n1h/VLMu2n1h), Ou2nbest(t-5m))
        let price_1hour: ExchangeRateValue = if one_hour_usn > U256::zero() {
            let multiplier = one_hour_usn
                * U256::from(10u128.pow(u32::from(DEFAULT_RATE_DECIMALS - USN_DECIMALS)))
                / one_hour_near;
            let new_price = ExchangeRateValue::new(multiplier.as_u128(), DEFAULT_RATE_DECIMALS);

            partial_min_max::max(new_price, best_prev_rate)
        } else {
            best_prev_rate
        };

        // Ou2nbest = max (ORACLE_smooth,ORACLE_current)
        // Pu2n(t)=($VLMu2nnow+$VLMu2n5m)/($VLMu2nnow+$VLMu2n1h)*Pu2n1h +
        //         ($VLMu2n1h—$VLMu2n5m)/($VLMu2nnow+$VLMu2n1h)*Ou2nbest
        let usn2near_best_rate =
            ExchangeRateValue::from(rates.max()).to_decimals(DEFAULT_RATE_DECIMALS);

        let a = (amount_usn + five_min_usn) * U256::from(price_1hour.multiplier())
            / (amount_usn + one_hour_usn);
        let b = (one_hour_usn - five_min_usn) * U256::from(usn2near_best_rate.multiplier())
            / (amount_usn + one_hour_usn);

        let multiplier = a + b;
        let price = ExchangeRate::new(multiplier.as_u128(), DEFAULT_RATE_DECIMALS);

        price
    }

    fn calculate_commission(
        &self,
        account_id: Option<AccountId>,
        amount_usn: u128,
        rate: &ExchangeRate,
    ) -> Commission {
        if let Some(account) = account_id {
            if account == self.owner_id.clone() {
                return Commission { usn: 0, near: 0 };
            }
        }
        let spread_denominator = 10u128.pow(SPREAD_DECIMAL as u32);
        let commission_usn =
            U256::from(amount_usn) * U256::from(self.spread_u128(amount_usn)) / spread_denominator; // amount * 0.005
        let commission_near = commission_usn
            * U256::from(10u128.pow(u32::from(rate.decimals() - USN_DECIMALS)))
            / rate.multiplier();

        Commission {
            usn: commission_usn.as_u128(),
            near: commission_near.as_u128(),
        }
    }

    pub fn predict_buy(
        &self,
        account_id: Option<AccountId>,
        amount: U128,
        rates: Vec<ExchangeRateValue>,
    ) -> ExchangeResultOutput {
        let amount = u128::from(amount);

        assert_ne!(amount, 0, "Amount can't be zero");
        assert_eq!(rates.len(), 2,
          "The rates list must contain exactly two values: spot and EMA, e.g. 'wrap.near' and 'wrap.near#3600' assets from priceoracle.near"
        );
        for &v in &rates {
            assert_ne!(v.multiplier(), 0, "Multiplier can't be zero");
            assert_ne!(v.decimals(), 0, "Decimals can't be zero");
        }

        let current_rate = ExchangeRate::from(rates[0]);
        let smooth_rate = ExchangeRate::from(rates[1]);
        let rates = ExchangeRates::new(current_rate, smooth_rate);
        let result = self.internal_predict_buy(account_id, amount, &rates, None);

        ExchangeResultOutput::from(result)
    }

    fn internal_predict_buy(
        &self,
        account_id: Option<AccountId>,
        amount: u128,
        rates: &ExchangeRates,
        expected: Option<ExpectedRate>,
    ) -> ExchangeResult {
        let near = U256::from(amount);

        let rate = self.buy_price(amount, &rates);
        if let Some(expected) = expected {
            Self::assert_exchange_rate(&rate, &expected);
        }

        // Make exchange: NEAR -> USN.
        let multiplier = U256::from(rate.multiplier());
        let amount = near * multiplier / 10u128.pow(u32::from(rate.decimals() - USN_DECIMALS));

        // Expected result (128-bit) can have 20 digits before and 18 after the decimal point.
        // We don't expect more than 10^20 tokens on a single account. It panics if overflows.
        let amount = amount.as_u128();

        // Commission.
        let commission = self.calculate_commission(account_id, amount, &rate);
        let price_with_fee = amount - commission.usn;

        ExchangeResult {
            commission: commission,
            amount: price_with_fee,
            rate: ExchangeRateValue::from(rate),
        }
    }

    pub fn predict_sell(
        &self,
        account_id: Option<AccountId>,
        amount: U128,
        rates: Vec<ExchangeRateValue>,
    ) -> ExchangeResultOutput {
        let amount = u128::from(amount);

        assert_ne!(amount, 0, "Amount can't be zero");
        assert_eq!(rates.len(), 2,
          "The rates list must contain exactly two values: spot and EMA, e.g. 'wrap.near' and 'wrap.near#3600' assets from priceoracle.near"
        );
        for &v in &rates {
            assert_ne!(v.multiplier(), 0, "Multiplier can't be zero");
            assert_ne!(v.decimals(), 0, "Decimals can't be zero");
        }

        let current_rate = ExchangeRate::from(rates[0]);
        let smooth_rate = ExchangeRate::from(rates[1]);
        let rates = ExchangeRates::new(current_rate, smooth_rate);
        let result = self.internal_predict_sell(account_id, amount, &rates, None);

        ExchangeResultOutput::from(result)
    }

    fn internal_predict_sell(
        &self,
        account_id: Option<AccountId>,
        amount: u128,
        rates: &ExchangeRates,
        expected: Option<ExpectedRate>,
    ) -> ExchangeResult {
        let rate = self.sell_price(amount, &rates);

        if let Some(expected) = expected {
            Self::assert_exchange_rate(&rate, &expected);
        }

        let multiplier = U256::from(rate.multiplier());
        let commission = self.calculate_commission(account_id, amount, &rate);

        let amount: U256 = U256::from(amount) - commission.usn;

        // Make exchange: USN -> NEAR.
        let price_with_fee = (amount
            * U256::from(10u128.pow(u32::from(rate.decimals() - USN_DECIMALS)))
            / multiplier)
            .as_u128();

        ExchangeResult {
            commission: commission,
            amount: price_with_fee,
            rate: ExchangeRateValue::from(rate),
        }
    }

    /// Buys USN tokens for NEAR tokens.
    /// Can make cross-contract call to an oracle.
    /// Returns amount of purchased USN tokens.
    /// NOTE: The method returns a promise, but SDK doesn't support clone on promise and we
    ///     want to return a promise in the middle.
    #[payable]
    pub fn buy(&mut self, expected: Option<ExpectedRate>, to: Option<AccountId>) {
        self.assert_owner();
        self.abort_if_pause();
        self.abort_if_blacklisted(&env::predecessor_account_id());

        let near = env::attached_deposit();

        // Select target account.
        let account = to.unwrap_or_else(env::predecessor_account_id);

        Oracle::get_exchange_rate_promise()
            .then(ext_self::buy_with_price_callback(
                account.clone(),
                near.into(),
                expected,
                env::current_account_id(),
                NO_DEPOSIT,
                GAS_FOR_BUY_PROMISE,
            ))
            // Returning callback promise, so the transaction will return the value or a failure.
            // But the refund will still happen.
            .as_return()
            .then(ext_self::handle_refund(
                env::predecessor_account_id(),
                near.into(),
                env::current_account_id(),
                NO_DEPOSIT,
                GAS_FOR_REFUND_PROMISE,
            ));
    }

    /// Completes the purchase (NEAR -> USN). It is called in 2 cases:
    /// 1. Direct call from the `buy` method if the exchange rate cache is valid.
    /// 2. Indirect callback from the cross-contract call after getting a fresh exchange rate.
    fn finish_buy(
        &mut self,
        account: AccountId,
        near: Balance,
        expected: Option<ExpectedRate>,
        rates: ExchangeRates,
    ) -> Balance {
        self.best_rate.add(&rates, env::block_timestamp());
        self.usn2near.refresh(env::block_timestamp());
        self.near2usn.refresh(env::block_timestamp());

        let result = self.internal_predict_buy(Some(account.clone()), near, &rates, expected);

        self.commission.usn += result.commission.usn;
        self.commission.near += result.commission.near;

        if result.amount == 0 {
            env::panic_str("Not enough NEAR: attached deposit exchanges to 0 tokens");
        }

        self.token.internal_deposit(&account, result.amount);

        event::emit::ft_mint(&account, result.amount, None);

        let amount_usn_with_fee = result.amount + result.commission.usn;
        self.near2usn
            .add(amount_usn_with_fee, near, env::block_timestamp());

        result.amount
    }

    /// Sells USN tokens getting NEAR tokens.
    /// Return amount of purchased NEAR tokens.
    #[payable]
    pub fn sell(&mut self, amount: U128, expected: Option<ExpectedRate>) -> Promise {
        assert_one_yocto();
        self.assert_owner();
        self.abort_if_pause();
        self.abort_if_blacklisted(&env::predecessor_account_id());

        let amount = Balance::from(amount);

        if amount == 0 {
            env::panic_str("Not allowed to sell 0 tokens");
        }

        let account = env::predecessor_account_id();

        Oracle::get_exchange_rate_promise().then(ext_self::sell_with_price_callback(
            account,
            amount.into(),
            expected,
            env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_SELL_PROMISE,
        ))
    }

    /// Finishes the sell (USN -> NEAR). It is called in 2 cases:
    /// 1. Direct call from the `sell` method if the exchange rate cache is valid.
    /// 2. Indirect callback from the cross-contract call after getting a fresh exchange rate.
    fn finish_sell(
        &mut self,
        account: AccountId,
        amount: Balance,
        expected: Option<ExpectedRate>,
        rates: ExchangeRates,
    ) -> Balance {
        self.best_rate.add(&rates, env::block_timestamp());
        self.usn2near.refresh(env::block_timestamp());
        self.near2usn.refresh(env::block_timestamp());

        let result = self.internal_predict_sell(Some(account.clone()), amount, &rates, expected);

        self.commission.usn += result.commission.usn;
        self.commission.near += result.commission.near;

        self.token.internal_withdraw(&account, amount);

        event::emit::ft_burn(&account, amount, None);

        let amount_near_with_fee = result.amount + result.commission.near;
        self.usn2near
            .add(amount, amount_near_with_fee, env::block_timestamp());

        result.amount
    }

    fn assert_exchange_rate(actual: &ExchangeRate, expected: &ExpectedRate) {
        let slippage = u128::from(expected.slippage);
        let multiplier = u128::from(expected.multiplier);
        let start = multiplier.saturating_sub(slippage);
        let end = multiplier.saturating_add(slippage);
        assert_eq!(
            actual.decimals(),
            expected.decimals,
            "Slippage error: different decimals"
        );

        if !(start..=end).contains(&actual.multiplier()) {
            env::panic_str(&format!(
                "Slippage error: fresh exchange rate {} is out of expected range {} +/- {}",
                actual.multiplier(),
                multiplier,
                slippage
            ));
        }
    }

    pub fn contract_status(&self) -> ContractStatus {
        self.status.clone()
    }

    /// Returns the name of the token.
    pub fn name(&self) -> String {
        let metadata = self.metadata.get();
        metadata.expect("Unable to get decimals").name
    }

    /// Returns the symbol of the token.
    pub fn symbol(&self) -> String {
        let metadata = self.metadata.get();
        metadata.expect("Unable to get decimals").symbol
    }

    /// Returns the decimals places of the token.
    pub fn decimals(&self) -> u8 {
        let metadata = self.metadata.get();
        metadata.expect("Unable to get decimals").decimals
    }

    /// Returns either a fixed spread, or a adaptive spread
    pub fn spread(&self, amount: Option<U128>) -> U128 {
        let amount = amount.unwrap_or(U128::from(0));
        self.spread_u128(u128::from(amount)).into()
    }

    fn spread_u128(&self, amount: u128) -> u128 {
        match &self.spread {
            Spread::Fixed(spread) => *spread,
            Spread::Exponential(params) => {
                // C1(v) = CHV + (CLV - CHV) * e ^ {-s1 * amount}
                //     CHV = 0.1%
                //     CLV = 0.5%
                //     s1 = 0.0000075
                //     amount in [1, 10_000_000]
                // Normalize amount to a number from $0 to $10,000,000.

                let decimals = 10u128.pow(USN_DECIMALS as u32);
                let n = amount / decimals; // [0, ...], dropping decimals
                let amount: u128 = if n > 10_000_000 { 10_000_000 } else { n }; // [0, 10_000_000]
                let exp = (-params.scaler * amount as f64).exp();
                let spread = params.min + (params.max - params.min) * exp;
                (spread * (10u32.pow(SPREAD_DECIMAL as u32) as f64)).round() as u128
            }
        }
    }

    pub fn set_fixed_spread(&mut self, spread: U128) {
        self.assert_owner();
        let spread = Balance::from(spread);
        self.spread = self.internal_set_fixed_spread(spread);
    }

    pub fn set_adaptive_spread(&mut self, params: Option<ExponentialSpreadParams>) {
        self.assert_owner();

        self.spread = match params {
            None => Spread::Exponential(ExponentialSpreadParams::default()),
            Some(params) => {
                let max_spread = MAX_SPREAD as f64 / 10u64.pow(SPREAD_DECIMAL as u32) as f64;
                if params.max == f64::NAN || params.min == f64::NAN || params.scaler == f64::NAN {
                    env::panic_str("params.min, params.max, params.scaler cannot be a NAN value");
                }
                if params.max == params.min {
                    let spread = params.min as u128;
                    self.internal_set_fixed_spread(spread)
                } else {
                    if params.max < params.min {
                        env::panic_str("params.min cannot be greater than params.max");
                    }
                    if params.max > max_spread {
                        env::panic_str(&format!("params.max is greater than {}", max_spread));
                    }
                    if params.scaler > SPREAD_MAX_SCALER {
                        #[rustfmt::skip]
                    env::panic_str(&format!("params.scaler is greater than {}", SPREAD_MAX_SCALER));
                    }
                    if params.min.is_sign_negative() || params.scaler.is_sign_negative() {
                        env::panic_str("params.min, params.scaler cannot be negative");
                    }
                    Spread::Exponential(params)
                }
            }
        }
    }

    fn internal_set_fixed_spread(&self, spread: u128) -> Spread {
        if spread > MAX_SPREAD {
            const PERCENT_MULTIPLICATOR: u128 = 100;

            env::panic_str(&format!(
                "Spread limit is {}%",
                MAX_SPREAD * PERCENT_MULTIPLICATOR / 10u128.pow(SPREAD_DECIMAL as u32)
            ));
        }
        Spread::Fixed(spread)
    }

    pub fn version(&self) -> String {
        format!("{}:{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
    }

    pub fn commission(&self) -> CommissionOutput {
        self.commission.clone().into()
    }

    pub fn treasury(&self) -> TreasuryData {
        self.treasury.get().expect("Valid treasury")
    }

    pub fn history(&self) -> HistoryData {
        HistoryData {
            usn2near: self.usn2near.clone(),
            near2usn: self.near2usn.clone(),
            best_rate: self.best_rate.clone(),
        }
    }

    /// This is NOOP implementation. KEEP IT if you haven't changed contract state.
    /// Should only be called by this contract on migration.
    /// This method is called from `upgrade()` method.
    /// For next version upgrades, change this function.
    #[init(ignore_state)]
    #[private]
    pub fn migrate() -> Self {
        #[derive(BorshDeserialize, BorshSerialize)]
        pub struct OldContract {
            owner_id: AccountId,
            proposed_owner_id: AccountId,
            guardians: UnorderedSet<AccountId>,
            token: FungibleTokenFreeStorage,
            metadata: LazyOption<FungibleTokenMetadata>,
            black_list: LookupMap<AccountId, BlackListStatus>,
            status: ContractStatus,
            oracle: Oracle,
            spread: Spread,
            commission: Commission,
            treasury: LazyOption<TreasuryData>,
            usn2near: VolumeHistory,
            near2usn: VolumeHistory,
            best_rate: MinMaxRate,
        }

        let contract: OldContract = env::state_read().expect("Contract is not initialized");

        Self {
            owner_id: contract.owner_id.clone(),
            proposed_owner_id: contract.owner_id,
            guardians: contract.guardians,
            token: contract.token,
            metadata: contract.metadata,
            black_list: contract.black_list,
            status: contract.status,
            oracle: contract.oracle,
            spread: contract.spread,
            commission: contract.commission,
            treasury: contract.treasury,
            usn2near: contract.usn2near,
            near2usn: contract.near2usn,
            best_rate: contract.best_rate,
            stable_treasury: StableTreasury::new(StorageKey::StableTreasury),
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
    pub fn withdraw(&mut self, token_id: Option<AccountId>, amount: U128) -> Promise {
        let account_id = env::predecessor_account_id();
        let token_id = token_id.unwrap_or(usdt_id());

        assert_one_yocto();
        self.abort_if_pause();
        self.abort_if_blacklisted(&account_id);

        let token_amount =
            self.stable_treasury
                .withdraw(&mut self.token, &account_id, &token_id, amount.into());

        ext_ft_api::ft_transfer(
            account_id.clone(),
            token_amount.into(),
            None,
            token_id.clone(),
            ONE_YOCTO,
            GAS_FOR_FT_TRANSFER,
        )
        .as_return()
        .then(ext_self::handle_withdraw_refund(
            account_id,
            token_id,
            amount,
            env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_REFUND_PROMISE,
        ))
    }

    pub fn add_stable_asset(&mut self, token_id: &AccountId, decimals: u8) {
        self.assert_owner();
        self.stable_treasury.add_token(token_id, decimals);
    }

    pub fn enable_stable_asset(&mut self, token_id: &AccountId) {
        self.assert_owner();
        self.stable_treasury.enable_token(token_id);
    }

    pub fn disable_stable_asset(&mut self, token_id: &AccountId) {
        self.assert_owner();
        self.stable_treasury.disable_token(token_id);
    }

    pub fn stable_assets(&self) -> Vec<(AccountId, StableInfo)> {
        self.stable_treasury.supported_tokens()
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use history::{FIVE_MINUTES, ONE_MINUTE};
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, Balance, ONE_NEAR, ONE_YOCTO};

    const ONE_USN: u128 = 1_000_000_000_000_000_000;

    use crate::history::ONE_HOUR;

    use super::*;

    impl From<ExchangeRate> for ExpectedRate {
        fn from(rate: ExchangeRate) -> Self {
            Self {
                multiplier: rate.multiplier().into(),
                slippage: (rate.multiplier() * 5 / 100).into(), // 5%
                decimals: rate.decimals(),
            }
        }
    }

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
    fn test_view_treasury() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = Contract::new(accounts(1));
        testing_env!(context.predecessor_account_id(accounts(1)).build());
        assert_eq!(contract.treasury(), TreasuryData::default());
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
    #[should_panic(expected = "This method can be called only by owner")]
    fn test_buy_sell() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());

        let mut contract = Contract::new(accounts(1));

        testing_env!(context.predecessor_account_id(accounts(2)).build());

        testing_env!(context
            .predecessor_account_id(accounts(2))
            .attached_deposit(ONE_NEAR)
            .build());

        let old_rate = ExchangeRate::test_old_rate();

        contract.buy(None, None);

        testing_env!(context.attached_deposit(ONE_YOCTO).build());

        contract.sell(U128::from(11032461000000000000), None);

        contract.buy(Some(old_rate.clone().into()), None);

        testing_env!(context.attached_deposit(ONE_YOCTO).build());

        let mut expected_rate: ExpectedRate = old_rate.clone().into();
        expected_rate.multiplier = (old_rate.multiplier() * 96 / 100).into();

        contract.sell(U128::from(9900000000000000000), Some(expected_rate));
    }

    #[test]
    #[should_panic(expected = "This method can be called only by owner")]
    fn test_buy_auto_registration() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());

        let mut contract = Contract::new(accounts(1));

        testing_env!(context.predecessor_account_id(accounts(2)).build());
        contract.buy(None, None);

        testing_env!(context.attached_deposit(ONE_YOCTO).build());

        contract.sell(U128::from(11032461000000000000), None);
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

    #[test]
    #[should_panic(expected = "This method can be called only by owner")]
    fn test_cannot_buy() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());

        let mut contract = Contract::new(accounts(1));

        contract.add_to_blacklist(&accounts(2)); // It'll cause panic on buy.

        testing_env!(context.predecessor_account_id(accounts(2)).build());

        testing_env!(context
            .predecessor_account_id(accounts(2))
            .attached_deposit(ONE_NEAR)
            .build());
        contract.buy(None, None);
    }

    #[test]
    #[should_panic(expected = "This method can be called only by owner")]
    fn test_cannot_sell() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());

        let mut contract = Contract::new(accounts(1));

        contract.add_to_blacklist(&accounts(2)); // It'll cause panic on sell.

        testing_env!(context.predecessor_account_id(accounts(2)).build());

        testing_env!(context
            .predecessor_account_id(accounts(2))
            .attached_deposit(ONE_YOCTO)
            .build());
        contract.sell(U128::from(1), Some(ExchangeRate::test_old_rate().into()));
    }

    #[test]
    fn test_fixed_spread() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1));

        contract.set_fixed_spread(MAX_SPREAD.into());
        assert_eq!(contract.spread(None).0, MAX_SPREAD);
        let res =
            std::panic::catch_unwind(move || contract.set_fixed_spread((MAX_SPREAD + 1).into()));
        assert!(res.is_err());
    }

    #[test]
    fn test_adaptive_spread() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1));

        let one_token = 10u128.pow(contract.decimals() as u32);
        let hundred_thousands = one_token * 100_000;
        let ten_mln = one_token * 10_000_000;

        contract.set_adaptive_spread(None);
        assert_eq!(contract.spread(Some(one_token.into())).0, 5000); // $1: 0.0050000 = 0.5%
        assert_eq!(contract.spread(Some(hundred_thousands.into())).0, 2889); // $1000: 0.002889 = 0.289%
        assert_eq!(contract.spread(Some(ten_mln.into())).0, 1000); // $10mln: 0.001000 = 0.1%

        contract.set_adaptive_spread(Some(ExponentialSpreadParams {
            min: 0.002,
            max: 0.006,
            scaler: 0.00001,
        }));
        assert_eq!(contract.spread(Some(one_token.into())).0, 6000); // $1: 0.0060000 = 0.6%
        assert_eq!(contract.spread(Some(hundred_thousands.into())).0, 3472); // $1000: 0.003472 = 0.347%
        assert_eq!(contract.spread(Some(ten_mln.into())).0, 2000); // $10mln: 0.002000 = 021%
    }

    #[test]
    #[should_panic]
    fn test_adaptive_spread_min_limit() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1));

        contract.set_adaptive_spread(Some(ExponentialSpreadParams {
            min: 0.06,
            max: 0.001,
            scaler: 0.00001,
        }));
    }

    #[test]
    #[should_panic]
    fn test_adaptive_spread_max_limit() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1));

        contract.set_adaptive_spread(Some(ExponentialSpreadParams {
            min: 0.001,
            max: 0.06,
            scaler: 0.00001,
        }));
    }

    #[test]
    #[should_panic]
    fn test_adaptive_spread_min_gt_max() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1));

        contract.set_adaptive_spread(Some(ExponentialSpreadParams {
            min: 0.002,
            max: 0.001,
            scaler: 0.00001,
        }));
    }

    #[test]
    #[should_panic]
    fn test_adaptive_spread_scaler_limit() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1));

        contract.set_adaptive_spread(Some(ExponentialSpreadParams {
            min: 0.001,
            max: 0.002,
            scaler: 0.5,
        }));
    }

    #[test]
    #[should_panic]
    fn test_adaptive_spread_negative_param() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1));

        contract.set_adaptive_spread(Some(ExponentialSpreadParams {
            min: -0.001,
            max: -0.002,
            scaler: 1.0,
        }));
    }

    #[test]
    fn test_calculate_commission() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = Contract::new(accounts(1));
        testing_env!(context.predecessor_account_id(accounts(2)).build());

        let fresh_rate = ExchangeRate::test_fresh_rate();
        let result =
            contract.calculate_commission(Some(accounts(2)), 100000000000000000000, &fresh_rate);

        assert_eq!(result.usn, 499700000000000000);
        assert_eq!(result.near, 44840675167580470032932);
    }

    #[test]
    fn test_calculate_commission_owner() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = Contract::new(accounts(1));

        let fresh_rate = ExchangeRate::test_fresh_rate();
        let result =
            contract.calculate_commission(Some(accounts(1)), 100000000000000000000, &fresh_rate);

        assert_eq!(result.usn, 0);
        assert_eq!(result.near, 0);
    }

    #[test]
    pub fn test_buy_sell_price() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1));

        //
        // Smoke test
        //
        let current1 = ExchangeRate::test_create_rate(82004, 28);
        let smooth1 = ExchangeRate::test_create_rate(82004, 28);
        let rates = ExchangeRates::new(current1, smooth1);

        assert_eq!(
            contract.finish_buy(
                accounts(1),
                1_234500000000000000000000, // 1.2345 NEAR
                None,
                rates.clone()
            ),
            10_123393800000000000 // 10.234 USN
        );
    }

    #[test]
    pub fn test_best_rate() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1));
        let current1 = ExchangeRate::test_create_rate(82004, 28);
        let smooth1 = ExchangeRate::test_create_rate(820141234, 32);
        contract.best_rate.add(
            &ExchangeRates::new(current1, smooth1),
            env::block_timestamp(),
        );

        // Change timestamp
        testing_env!(context
            .block_timestamp(env::block_timestamp() + FIVE_MINUTES + ONE_MINUTE)
            .build());

        let current1 = ExchangeRate::test_create_rate(82004, 28);
        let smooth1 = ExchangeRate::test_create_rate(82014, 28);
        let rates = ExchangeRates::new(current1, smooth1);

        assert_eq!(
            contract.finish_buy(accounts(1), 1_234500000000000000000000, None, rates.clone()),
            10_123393800000000000
        );

        assert_eq!(
            contract.finish_sell(accounts(1), 10 * ONE_USN, None, rates.clone()),
            1219304021264662130855707
        );
    }

    #[test]
    pub fn test_history() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1));

        let current1 = ExchangeRate::test_create_rate(82004, 28);
        let smooth1 = ExchangeRate::test_create_rate(82014, 28);
        let rates = ExchangeRates::new(current1, smooth1);

        let fresh_rates = ExchangeRates::test_fresh_rate();
        let expected_rate: ExpectedRate = fresh_rates.clone().current.into();

        // History is empty
        for n in 0..10 {
            assert_eq!(
                contract.finish_buy(
                    accounts(1),
                    1_000 * ONE_NEAR,
                    Some(expected_rate.clone()),
                    fresh_rates.clone()
                ),
                11143900000000000000000,
                "interation is {}",
                n
            );
        }
        assert_eq!(contract.near2usn.one_hour.sum_near(), 1_000 * ONE_NEAR * 10);
        assert_eq!(
            contract.near2usn.one_hour.sum_usn(),
            11143900000000000000000 * 10
        );
        assert_eq!(contract.near2usn.five_min.sum_near(), 1_000 * ONE_NEAR * 10);
        assert_eq!(
            contract.near2usn.five_min.sum_usn(),
            11143900000000000000000 * 10
        );

        for n in 0..10 {
            assert_eq!(
                contract.finish_sell(
                    accounts(1),
                    1_000 * ONE_USN,
                    Some(expected_rate.clone()),
                    fresh_rates.clone()
                ),
                89735191450030958641050260,
                "interation is {}",
                n
            );
        }
        assert_eq!(
            contract.usn2near.one_hour.sum_near(),
            89735191450030958641050260 * 10
        );
        assert_eq!(contract.usn2near.one_hour.sum_usn(), 1_000 * ONE_USN * 10);
        assert_eq!(
            contract.usn2near.five_min.sum_near(),
            89735191450030958641050260 * 10
        );
        assert_eq!(contract.usn2near.five_min.sum_usn(), 1_000 * ONE_USN * 10);

        // Change timestamp
        testing_env!(context
            .block_timestamp(env::block_timestamp() + FIVE_MINUTES + ONE_MINUTE)
            .build());

        // Test with updated history
        assert_eq!(
            contract.finish_buy(
                accounts(1),
                1_000 * ONE_NEAR,
                Some(expected_rate.clone()),
                fresh_rates.clone()
            ),
            11143800000000000000000
        );
        assert_eq!(
            contract.finish_buy(
                accounts(1),
                1_000 * ONE_NEAR,
                Some(expected_rate.clone()),
                fresh_rates.clone()
            ),
            11143800000000000000000
        );
        assert_eq!(
            contract.near2usn.one_hour.sum_near(),
            2 * 1_000 * ONE_NEAR + 1_000 * ONE_NEAR * 10
        );
        assert_eq!(
            contract.near2usn.one_hour.sum_usn(),
            2 * 11143800000000000000000 + 11143900000000000000000 * 10
        );
        assert_eq!(contract.near2usn.five_min.sum_near(), 2 * 1_000 * ONE_NEAR);
        assert_eq!(
            contract.near2usn.five_min.sum_usn(),
            11143800000000000000000 + 11143800000000000000000
        );

        assert_eq!(
            contract.finish_sell(
                accounts(1),
                1_000 * ONE_USN,
                Some(expected_rate.clone()),
                fresh_rates.clone()
            ),
            89735996697715321524076167
        );
        assert_eq!(
            contract.finish_sell(
                accounts(1),
                1_000 * ONE_USN,
                Some(expected_rate.clone()),
                fresh_rates.clone()
            ),
            89735996697715321524076167
        );
        assert_eq!(
            contract.usn2near.one_hour.sum_near(),
            89735191450030958641050260 * 10 + 2 * 89735996697715321524076167
        );
        assert_eq!(contract.usn2near.one_hour.sum_usn(), 1_000 * ONE_USN * 12);
        assert_eq!(
            contract.usn2near.five_min.sum_near(),
            2 * 89735996697715321524076167
        );
        assert_eq!(contract.usn2near.five_min.sum_usn(), 1_000 * ONE_USN * 2);

        // Change timestamp
        testing_env!(context
            .block_timestamp(env::block_timestamp() + ONE_MINUTE + 1)
            .build());
        assert_eq!(
            contract.finish_buy(
                accounts(1),
                1_000 * ONE_NEAR,
                Some(expected_rate.clone()),
                fresh_rates.clone()
            ),
            11143800000000000000000
        );
        assert_eq!(
            contract.finish_buy(
                accounts(1),
                1_000 * ONE_NEAR,
                Some(expected_rate.clone()),
                fresh_rates.clone()
            ),
            11143800000000000000000
        );

        assert_eq!(
            contract.finish_sell(
                accounts(1),
                1_000 * ONE_USN,
                Some(expected_rate.clone()),
                fresh_rates.clone()
            ),
            89735996697715321524076167
        );
        assert_eq!(
            contract.finish_sell(
                accounts(1),
                1_000 * ONE_USN,
                Some(expected_rate.clone()),
                fresh_rates.clone()
            ),
            89735996697715321524076167
        );

        assert_eq!(
            contract.finish_buy(
                accounts(1),
                1_234500000000000000000000, // 1.2345 NEAR
                None,
                rates.clone()
            ),
            11_161731750000000000
        );

        assert_eq!(
            contract.finish_sell(
                accounts(1),
                ONE_USN,
                Some(expected_rate.clone()),
                fresh_rates.clone()
            ),
            89735996697715321524076 // 0.0897
        );

        // Change the timestamp +1 hour
        // The history must be clear
        testing_env!(context
            .block_timestamp(env::block_timestamp() + ONE_HOUR + 1)
            .build());

        let current1 = ExchangeRate::test_create_rate(52004, 28);
        let smooth1 = ExchangeRate::test_create_rate(52004, 28);
        let rates = ExchangeRates::new(current1, smooth1);

        assert_eq!(
            contract.finish_buy(
                accounts(1),
                1_234500000000000000000000, // 1.2345 NEAR
                None,
                rates.clone()
            ),
            6_419893800000000000
        );
    }

    #[test]
    fn test_u256() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());

        let mut contract = Contract::new(accounts(1));

        testing_env!(context.predecessor_account_id(accounts(2)).build());

        let fresh_rates = ExchangeRates::test_fresh_rate();
        let expected_rate: ExpectedRate = fresh_rates.clone().current.into();

        assert_eq!(
            contract.finish_buy(
                accounts(2),
                1_000_000_000_000 * ONE_NEAR,
                Some(expected_rate.clone()),
                fresh_rates.clone(),
            ),
            11132756100000_000000000000000000
        );

        testing_env!(context
            .account_balance(1_000_000_000_000 * ONE_NEAR)
            .build());

        assert_eq!(
            contract.finish_sell(
                accounts(2),
                11088180500000_000000000000000000,
                Some(expected_rate),
                fresh_rates,
            ),
            994_005_000000000000000000000000000000
        );
    }

    #[test]
    fn test_owner_buy_sell() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());

        let mut contract = Contract::new(accounts(1));

        testing_env!(context.predecessor_account_id(accounts(1)).build());

        let fresh_rates = ExchangeRates::test_fresh_rate();
        let expected_rate: ExpectedRate = fresh_rates.clone().current.into();

        assert_eq!(
            contract.finish_buy(
                accounts(1),
                1_000_000_000_000 * ONE_NEAR,
                Some(expected_rate.clone()),
                fresh_rates.clone(),
            ),
            11143900000000_000000000000000000
        );

        testing_env!(context
            .account_balance(1_000_000_000_000 * ONE_NEAR)
            .build());

        assert_eq!(
            contract.finish_sell(
                accounts(1),
                11088180500000_000000000000000000,
                Some(expected_rate),
                fresh_rates,
            ),
            995_000_000000000000000000000000000000
        );
    }

    #[test]
    fn test_commission() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());

        let mut contract = Contract::new(accounts(1));

        testing_env!(context.predecessor_account_id(accounts(2)).build());

        let fresh_rates = ExchangeRates::test_fresh_rate();
        let expected_rate: ExpectedRate = fresh_rates.clone().current.into();

        contract.finish_buy(
            accounts(2),
            1 * ONE_NEAR,
            Some(expected_rate.clone()),
            fresh_rates.clone(),
        );
        assert_eq!(contract.commission().usn, U128(55_719_500_000_000_000));

        assert_eq!(
            contract.commission().near,
            U128(5_000_000_000_000_000_000_000)
        );

        contract.finish_sell(
            accounts(2),
            1_000_000_000_000_000,
            Some(expected_rate.clone()),
            fresh_rates,
        );
        assert_eq!(contract.commission().usn, U128(55_724_500_000_000_000));

        assert_eq!(
            contract.commission().near,
            U128(5_000_448_675_957_250_154_793)
        );
    }

    #[test]
    #[should_panic(expected = "The account doesn't have enough balance")]
    fn test_not_enough_deposit() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());

        let mut contract = Contract::new(accounts(1));

        testing_env!(context.predecessor_account_id(accounts(2)).build());

        let fresh_rates = ExchangeRates::test_fresh_rate();
        let expected_rate: ExpectedRate = fresh_rates.clone().current.into();

        contract.finish_sell(
            accounts(2),
            11032461000000_000000000000000000,
            Some(expected_rate),
            fresh_rates,
        );
    }
}
