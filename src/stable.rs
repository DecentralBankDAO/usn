use crate::*;

use near_sdk::{collections::UnorderedMap, IntoStorageKey};

const PERCENT_MULTIPLICATOR: u128 = 100;
const USDT_DECIMALS: u8 = 6;
const MAX_VALID_DECIMALS: u8 = 37;
const MAX_COMMISSION_RATE: u32 = 50000; // 0.05 = 5%
const SPREAD_DECIMAL: u8 = 6;
const INITIAL_COMMISSION_RATE: u32 = 100; // 0.0001 = 0.01%

pub fn usdt_id() -> AccountId {
    if cfg!(feature = "mainnet") {
        "dac17f958d2ee523a2206206994597c13d831ec7.factory.bridge.near"
    } else if cfg!(feature = "testnet") {
        "usdt.fakes.testnet"
    } else {
        "usdt.test.near"
    }
    .parse()
    .unwrap()
}

#[derive(BorshDeserialize, BorshSerialize, PartialEq, Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub enum AssetStatus {
    Enabled,
    Disabled,
}

#[derive(Debug)]
pub enum AssetAction {
    Deposit,
    Withdraw,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct CommissionRate {
    deposit: Option<u32>,
    withdraw: Option<u32>,
}

impl Default for CommissionRate {
    fn default() -> Self {
        Self {
            deposit: Some(INITIAL_COMMISSION_RATE),
            withdraw: Some(INITIAL_COMMISSION_RATE),
        }
    }
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct OldAssetInfo {
    decimals: u8,
    commission: U128,
    status: AssetStatus,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct AssetInfo {
    decimals: u8,
    status: AssetStatus,
    // Stored in USN due to more precise value
    commission: U128,
    commission_rate: CommissionRate,
}

impl AssetInfo {
    pub fn new(decimals: u8) -> Self {
        assert!(
            decimals > 0 && decimals <= MAX_VALID_DECIMALS,
            "Decimal value is out of bounds"
        );

        AssetInfo {
            decimals,
            status: AssetStatus::Enabled,
            commission: U128(0),
            commission_rate: CommissionRate::default(),
        }
    }

    pub fn commission(&self) -> U128 {
        self.commission
    }
}

fn copy_asset_info(old_asset_info: &OldAssetInfo) -> AssetInfo {
    AssetInfo {
        decimals: old_asset_info.decimals,
        status: old_asset_info.status.clone(),
        commission: old_asset_info.commission,
        commission_rate: CommissionRate::default(),
    }
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct OldStableTreasury {
    stable_token: UnorderedMap<AccountId, OldAssetInfo>,
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct StableTreasury {
    assets: UnorderedMap<AccountId, AssetInfo>,
}

impl StableTreasury {
    pub fn new<S>(prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        let mut this = Self {
            assets: UnorderedMap::new(prefix),
        };

        // USDT is supported by default.
        this.add_asset(&usdt_id(), USDT_DECIMALS);
        this
    }

    pub fn add_asset(&mut self, asset_id: &AccountId, decimals: u8) {
        assert!(
            self.assets.get(asset_id).is_none(),
            "Stable asset {} is already supported",
            asset_id
        );
        let asset_info = AssetInfo::new(decimals);
        self.assets.insert(asset_id, &asset_info);
    }

    pub fn enable_asset(&mut self, asset_id: &AccountId) {
        self.assert_asset(asset_id);
        self.assert_status(asset_id, AssetStatus::Disabled);
        self.switch_status(asset_id, AssetStatus::Enabled);
    }

    pub fn disable_asset(&mut self, asset_id: &AccountId) {
        self.assert_asset(asset_id);
        self.assert_status(asset_id, AssetStatus::Enabled);
        self.switch_status(asset_id, AssetStatus::Disabled);
    }

    fn switch_status(&mut self, asset_id: &AccountId, status: AssetStatus) {
        let mut asset_info = self.assets.get(asset_id).unwrap();
        asset_info.status = status;
        self.assets.insert(asset_id, &asset_info);
    }

    pub fn supported_assets(&self) -> Vec<(AccountId, AssetInfo)> {
        self.assets.to_vec()
    }

    pub fn deposit(
        &mut self,
        ft: &mut FungibleTokenFreeStorage,
        account_id: &AccountId,
        asset_id: &AccountId,
        asset_amount: Balance,
    ) {
        self.assert_asset(asset_id);
        self.assert_status(asset_id, AssetStatus::Enabled);
        let asset = self.assets.get(asset_id).unwrap();
        let amount = self.convert_decimals(asset_amount, asset.decimals, USN_DECIMALS);
        let amount_without_fee = self.withdraw_commission(asset_id, amount, AssetAction::Deposit);
        ft.internal_deposit(account_id, amount_without_fee);
        event::emit::ft_mint(account_id, amount_without_fee, None);
    }

    pub fn withdraw(
        &mut self,
        ft: &mut FungibleTokenFreeStorage,
        account_id: &AccountId,
        asset_id: &AccountId,
        amount: Balance,
    ) -> u128 {
        self.assert_asset(asset_id);
        self.assert_status(asset_id, AssetStatus::Enabled);
        let asset = self.assets.get(asset_id).unwrap();
        let amount_without_fee = self.withdraw_commission(asset_id, amount, AssetAction::Withdraw);
        let asset_amount = self.convert_decimals(amount_without_fee, USN_DECIMALS, asset.decimals);
        assert_ne!(
            asset_amount, 0,
            "Not enough USN: specified amount exchanges to 0 tokens"
        );
        ft.internal_withdraw(account_id, amount);
        event::emit::ft_burn(account_id, amount, None);
        asset_amount
    }

    pub fn refund(
        &mut self,
        ft: &mut FungibleTokenFreeStorage,
        account_id: &AccountId,
        asset_id: &AccountId,
        original_amount: Balance,
    ) {
        self.assert_asset(asset_id);
        self.assert_status(asset_id, AssetStatus::Enabled);
        self.refund_commission(asset_id, original_amount);
        ft.internal_deposit(account_id, original_amount);
        event::emit::ft_mint(account_id, original_amount, Some("Refund"));
    }

    fn convert_decimals(&self, amount: u128, decimals_from: u8, decimals_to: u8) -> u128 {
        if decimals_from < decimals_to {
            amount * 10u128.pow(u32::from(decimals_to - decimals_from))
        } else if decimals_from > decimals_to {
            amount / 10u128.pow(u32::from(decimals_from - decimals_to))
        } else {
            amount
        }
    }

    fn assert_asset(&self, asset_id: &AccountId) {
        if !self.assets.get(asset_id).is_some() {
            env::panic_str(&format!("Asset {} is not supported", asset_id));
        }
    }

    fn assert_status(&self, asset_id: &AccountId, status: AssetStatus) {
        if self.assets.get(asset_id).unwrap().status != status {
            env::panic_str(&format!("Asset {} is currently not {:?}", asset_id, status));
        }
    }

    fn withdraw_commission(
        &mut self,
        asset_id: &AccountId,
        amount: u128,
        action: AssetAction,
    ) -> u128 {
        let mut asset_info = self.assets.get(asset_id).unwrap();
        let commission_rate = match action {
            AssetAction::Deposit => asset_info.commission_rate.deposit.unwrap(),
            AssetAction::Withdraw => asset_info.commission_rate.withdraw.unwrap(),
        };
        let commission = amount * commission_rate as u128 / 10u128.pow(SPREAD_DECIMAL as u32);
        asset_info.commission = (asset_info.commission.0 + commission).into();
        self.assets.insert(asset_id, &asset_info);

        amount - commission
    }

    fn refund_commission(&mut self, asset_id: &AccountId, amount: u128) {
        let asset_info = self.assets.get(asset_id).unwrap();
        let commission = amount * asset_info.commission_rate.withdraw.unwrap() as u128
            / 10u128.pow(SPREAD_DECIMAL as u32);
        self.decrease_commission(asset_id, commission);
    }

    pub fn decrease_commission(&mut self, asset_id: &AccountId, commission: u128) {
        let mut asset_info = self.assets.get(asset_id).unwrap();
        if let Some(commission) = asset_info.commission.0.checked_sub(commission) {
            asset_info.commission = commission.into();
        } else {
            env::panic_str(&format!("Failed to decrease asset {} commission", asset_id));
        }
        self.assets.insert(asset_id, &asset_info);
    }

    pub fn set_commission_rate(&mut self, asset_id: &AccountId, rate: CommissionRate) {
        self.assert_asset(asset_id);

        let mut asset_info = self.assets.get(asset_id).unwrap();
        if let Some(deposit_rate) = rate.deposit {
            self.assert_rate(deposit_rate);
            asset_info.commission_rate.deposit = Some(deposit_rate);
            self.new_rate_log(AssetAction::Deposit, deposit_rate);
        }
        if let Some(withdraw_rate) = rate.withdraw {
            self.assert_rate(withdraw_rate);
            asset_info.commission_rate.withdraw = Some(withdraw_rate);
            self.new_rate_log(AssetAction::Withdraw, withdraw_rate);
        }
        self.assets.insert(asset_id, &asset_info);
    }

    fn assert_rate(&self, rate: u32) {
        assert!(
            rate <= MAX_COMMISSION_RATE,
            "Commission rate cannot be more than 5%"
        );
    }

    fn new_rate_log(&self, action: AssetAction, rate: u32) {
        env::log_str(&format!(
            "New {:?} commission rate was set: {}%",
            action,
            rate as f64 * PERCENT_MULTIPLICATOR as f64 / 10f64.powi(SPREAD_DECIMAL as i32)
        ));
    }

    pub fn commission_rate(&self, asset_id: &AccountId) -> CommissionRate {
        self.assert_asset(asset_id);
        let asset_info = self.assets.get(asset_id).unwrap();
        asset_info.commission_rate
    }

    pub fn from_old<S>(old_treasury: &mut OldStableTreasury, prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        let old_assets = old_treasury.stable_token.to_vec();
        old_treasury.stable_token.clear();
        let mut new_assets = UnorderedMap::new(prefix);
        for (asset_id, old_asset_info) in old_assets.iter() {
            new_assets.insert(&asset_id.clone(), &copy_asset_info(old_asset_info));
        }
        StableTreasury { assets: new_assets }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use near_sdk::test_utils::accounts;

    #[test]
    fn test_stable_assets() {
        let treasury = StableTreasury::new(StorageKey::StableTreasury);
        assert_eq!(treasury.supported_assets()[0].0, usdt_id());
        assert_eq!(treasury.supported_assets()[0].1.decimals, 6);
    }

    #[test]
    fn test_supported_assets() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        treasury.add_asset(&accounts(1), 20);
        assert!(treasury.assets.get(&accounts(1)).is_some());
    }

    #[test]
    #[should_panic(expected = "Stable asset bob is already supported")]
    fn test_add_asset_twice() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        treasury.add_asset(&accounts(1), 20);
        assert!(treasury.assets.get(&accounts(1)).is_some());
        treasury.add_asset(&accounts(1), 20);
    }

    #[test]
    #[should_panic(expected = "Decimal value is out of bounds")]
    fn test_add_asset_with_zero_decimals() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        treasury.add_asset(&accounts(1), 0);
    }

    #[test]
    #[should_panic(expected = "Decimal value is out of bounds")]
    fn test_add_asset_with_exceeded_decimals() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        treasury.add_asset(&accounts(1), MAX_VALID_DECIMALS + 1);
    }

    #[test]
    fn test_enable_disable_assets() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        treasury.add_asset(&accounts(1), 20);
        assert_eq!(
            treasury.supported_assets()[1].1.status,
            AssetStatus::Enabled
        );
        treasury.disable_asset(&accounts(1));
        assert_eq!(
            treasury.supported_assets()[1].1.status,
            AssetStatus::Disabled
        );
        treasury.enable_asset(&accounts(1));
        assert_eq!(
            treasury.supported_assets()[1].1.status,
            AssetStatus::Enabled
        );
    }

    #[test]
    #[should_panic(expected = "Asset bob is currently not Enabled")]
    fn test_disable_asset_twice() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        treasury.add_asset(&accounts(1), 20);
        assert_eq!(
            treasury.supported_assets()[1].1.status,
            AssetStatus::Enabled
        );
        treasury.disable_asset(&accounts(1));
        assert_eq!(
            treasury.supported_assets()[1].1.status,
            AssetStatus::Disabled
        );
        treasury.disable_asset(&accounts(1));
    }

    #[test]
    #[should_panic(expected = "Asset bob is currently not Disabled")]
    fn test_enable_asset_twice() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        treasury.add_asset(&accounts(1), 20);
        assert_eq!(
            treasury.supported_assets()[1].1.status,
            AssetStatus::Enabled
        );
        treasury.enable_asset(&accounts(1));
    }

    #[test]
    fn test_view_supported_assets() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        treasury.add_asset(&accounts(1), 20);
        assert_eq!(treasury.supported_assets().len(), 2);
        assert_eq!(treasury.supported_assets()[1].0, accounts(1));
    }

    #[test]
    fn test_convert_decimals_down() {
        let treasury = StableTreasury::new(StorageKey::StableTreasury);
        let amount = 10000;
        let asset_decimals = 20;
        let usn_amount = treasury.convert_decimals(amount, asset_decimals, USN_DECIMALS);
        assert_eq!(usn_amount, 100);
        let stable_amount = treasury.convert_decimals(usn_amount, USN_DECIMALS, asset_decimals);
        assert_eq!(stable_amount, amount);
    }

    #[test]
    fn test_convert_decimals_up() {
        let treasury = StableTreasury::new(StorageKey::StableTreasury);
        let amount = 10000;
        let asset_decimals = 16;
        let usn_amount = treasury.convert_decimals(amount, asset_decimals, USN_DECIMALS);
        assert_eq!(usn_amount, 1000000);
        let stable_amount = treasury.convert_decimals(usn_amount, USN_DECIMALS, asset_decimals);
        assert_eq!(stable_amount, amount);
    }

    #[test]
    fn test_convert_decimals_same() {
        let treasury = StableTreasury::new(StorageKey::StableTreasury);
        let amount = 10000;
        let asset_decimals = 18;
        let usn_amount = treasury.convert_decimals(amount, asset_decimals, USN_DECIMALS);
        assert_eq!(usn_amount, amount);
        let stable_amount = treasury.convert_decimals(usn_amount, USN_DECIMALS, asset_decimals);
        assert_eq!(stable_amount, amount);
    }

    #[test]
    #[should_panic(expected = "Asset charlie is not supported")]
    fn test_deposit_not_supported_asset() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let mut token = FungibleTokenFreeStorage::new(StorageKey::Token);
        treasury.deposit(&mut token, &accounts(1), &accounts(2), 10000);
    }

    #[test]
    #[should_panic(expected = "Asset usdt.test.near is currently not Enabled")]
    fn test_deposit_not_active_asset() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let mut token = FungibleTokenFreeStorage::new(StorageKey::Token);
        treasury.disable_asset(&usdt_id());
        treasury.deposit(&mut token, &accounts(2), &usdt_id(), 10000);
    }

    #[test]
    fn test_deposit() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let mut token = FungibleTokenFreeStorage::new(StorageKey::Token);

        treasury.add_asset(&accounts(2), 20);
        treasury.deposit(&mut token, &accounts(1), &accounts(2), 1000000);
        assert_eq!(treasury.supported_assets()[1].1.commission, U128(1));
        assert_eq!(token.accounts.get(&accounts(1)).unwrap(), 9999);
    }

    #[test]
    fn test_deposit_with_max_commission_rate() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let mut token = FungibleTokenFreeStorage::new(StorageKey::Token);

        treasury.set_commission_rate(
            &usdt_id(),
            CommissionRate {
                deposit: Some(MAX_COMMISSION_RATE),
                withdraw: None,
            },
        );
        assert_eq!(
            treasury.commission_rate(&usdt_id()).deposit,
            Some(MAX_COMMISSION_RATE)
        );
        treasury.deposit(&mut token, &accounts(1), &usdt_id(), 10000);
        assert_eq!(
            treasury.supported_assets()[0].1.commission,
            U128(500000000000000)
        );
        assert_eq!(token.accounts.get(&accounts(1)).unwrap(), 9500000000000000);
    }

    #[test]
    fn test_withdraw() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let mut token = FungibleTokenFreeStorage::new(StorageKey::Token);

        treasury.add_asset(&accounts(2), 8);
        treasury.deposit(&mut token, &accounts(1), &accounts(2), 100000);
        let usn_amount = token.accounts.get(&accounts(1)).unwrap();
        assert_eq!(usn_amount, 999900000000000);
        assert_eq!(
            treasury.supported_assets()[1].1.commission,
            U128(100000000000)
        );

        let withdrawn = treasury.withdraw(&mut token, &accounts(1), &accounts(2), usn_amount);
        assert_eq!(
            treasury.supported_assets()[1].1.commission,
            U128(199990000000)
        );
        assert!(token.accounts.get(&accounts(1)).is_none());
        assert_eq!(withdrawn, 99980);
    }

    #[test]
    fn test_withdraw_with_max_commission_rate() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let mut token = FungibleTokenFreeStorage::new(StorageKey::Token);

        treasury.add_asset(&accounts(2), 8);
        treasury.set_commission_rate(
            &accounts(2),
            CommissionRate {
                deposit: None,
                withdraw: Some(MAX_COMMISSION_RATE),
            },
        );
        assert_eq!(
            treasury.commission_rate(&accounts(2)).withdraw,
            Some(MAX_COMMISSION_RATE)
        );

        treasury.deposit(&mut token, &accounts(1), &accounts(2), 100000);
        let usn_amount = token.accounts.get(&accounts(1)).unwrap();
        assert_eq!(usn_amount, 999900000000000);
        assert_eq!(
            treasury.supported_assets()[1].1.commission,
            U128(100000000000)
        );

        let withdrawn = treasury.withdraw(&mut token, &accounts(1), &accounts(2), usn_amount);
        assert_eq!(
            treasury.supported_assets()[1].1.commission,
            U128(50095000000000)
        );
        assert!(token.accounts.get(&accounts(1)).is_none());
        assert_eq!(withdrawn, 94990);
    }

    #[test]
    fn test_deposit_withdraw_different_assets() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let mut token = FungibleTokenFreeStorage::new(StorageKey::Token);

        treasury.deposit(&mut token, &accounts(1), &usdt_id(), 100000);
        let usn_amount = token.accounts.get(&accounts(1)).unwrap();
        assert_eq!(usn_amount, 99990000000000000);
        assert_eq!(
            treasury.supported_assets()[0].1.commission,
            U128(10000000000000)
        );

        treasury.add_asset(&accounts(2), 8);
        treasury.set_commission_rate(
            &accounts(2),
            CommissionRate {
                deposit: None,
                withdraw: Some(5000),
            },
        );

        let withdrawn = treasury.withdraw(&mut token, &accounts(1), &accounts(2), usn_amount);
        assert_eq!(
            treasury.supported_assets()[1].1.commission,
            U128(499950000000000)
        );
        assert!(token.accounts.get(&accounts(1)).is_none());
        assert_eq!(withdrawn, 9949005);
    }

    #[test]
    fn test_refund() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let mut token = FungibleTokenFreeStorage::new(StorageKey::Token);

        treasury.deposit(&mut token, &accounts(1), &usdt_id(), 1000);
        let usn_amount = token.accounts.get(&accounts(1)).unwrap();
        assert_eq!(usn_amount, 999900000000000);
        assert_eq!(
            treasury.supported_assets()[0].1.commission,
            U128(100000000000)
        );

        let withdrawn = treasury.withdraw(&mut token, &accounts(1), &usdt_id(), usn_amount);
        assert_eq!(
            treasury.supported_assets()[0].1.commission,
            U128(199990000000)
        );
        assert!(token.accounts.get(&accounts(1)).is_none());
        assert_eq!(withdrawn, 999);

        treasury.refund(&mut token, &accounts(1), &usdt_id(), usn_amount);
        let usn_amount = token.accounts.get(&accounts(1)).unwrap();
        assert_eq!(usn_amount, 999900000000000);
        assert_eq!(
            treasury.supported_assets()[0].1.commission,
            U128(100000000000)
        );
    }

    #[test]
    fn test_refund_different_assets() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let mut token = FungibleTokenFreeStorage::new(StorageKey::Token);

        treasury.deposit(&mut token, &accounts(1), &usdt_id(), 1000);
        let usn_amount = token.accounts.get(&accounts(1)).unwrap();
        assert_eq!(usn_amount, 999900000000000);
        assert_eq!(
            treasury.supported_assets()[0].1.commission,
            U128(100000000000)
        );

        treasury.add_asset(&accounts(2), 8);

        treasury.set_commission_rate(
            &accounts(2),
            CommissionRate {
                deposit: None,
                withdraw: Some(5000),
            },
        );

        let withdrawn = treasury.withdraw(&mut token, &accounts(1), &accounts(2), usn_amount);
        assert_eq!(
            treasury.supported_assets()[1].1.commission,
            U128(4999500000000)
        );
        assert!(token.accounts.get(&accounts(1)).is_none());
        assert_eq!(withdrawn, 99490);

        treasury.refund(&mut token, &accounts(1), &accounts(2), usn_amount);
        let usn_amount = token.accounts.get(&accounts(1)).unwrap();
        assert_eq!(usn_amount, 999900000000000);
        assert_eq!(
            treasury.supported_assets()[0].1.commission,
            U128(100000000000)
        );
        assert_eq!(treasury.supported_assets()[1].1.commission, U128(0));
    }

    #[test]
    #[should_panic(expected = "Not enough USN: specified amount exchanges to 0 tokens")]
    fn test_conversion_loss() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let mut token = FungibleTokenFreeStorage::new(StorageKey::Token);

        treasury.add_asset(&accounts(2), 8);
        treasury.deposit(&mut token, &accounts(1), &accounts(2), 100000);
        assert_eq!(token.accounts.get(&accounts(1)).unwrap(), 999900000000000);

        token.internal_withdraw(&accounts(1), 1000);

        treasury.withdraw(&mut token, &accounts(1), &accounts(2), 9000000000);
    }

    #[test]
    #[should_panic(expected = "The account doesn't have enough balance")]
    fn test_withdraw_more() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let mut token = FungibleTokenFreeStorage::new(StorageKey::Token);

        treasury.add_asset(&accounts(2), 8);
        treasury.deposit(&mut token, &accounts(1), &accounts(2), 100000);
        let usn_amount = token.accounts.get(&accounts(1)).unwrap();
        token.internal_withdraw(&accounts(1), 1000);

        treasury.withdraw(&mut token, &accounts(1), &accounts(2), usn_amount);
    }

    #[test]
    fn test_withdraw_commission_different_actions() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        treasury.set_commission_rate(
            &usdt_id(),
            CommissionRate {
                deposit: Some(1000),
                withdraw: Some(2000),
            },
        );
        let amount_without_fee =
            treasury.withdraw_commission(&usdt_id(), 100000, AssetAction::Deposit);
        assert_eq!(amount_without_fee, 99900);
        assert_eq!(treasury.supported_assets()[0].1.commission, U128(100));
        let amount_without_fee =
            treasury.withdraw_commission(&usdt_id(), amount_without_fee, AssetAction::Withdraw);
        assert_eq!(amount_without_fee, 99701);
        assert_eq!(treasury.supported_assets()[0].1.commission, U128(299));
    }

    #[test]
    fn test_refund_commission() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        treasury.withdraw_commission(&usdt_id(), 100000000000000000000, AssetAction::Withdraw);
        assert_eq!(
            treasury.supported_assets()[0].1.commission.0,
            10000000000000000
        );
        treasury.refund_commission(&usdt_id(), 100000000000000000000);
        assert_eq!(treasury.supported_assets()[0].1.commission.0, 0);
    }

    #[test]
    fn test_set_commission_rate() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        assert_eq!(treasury.commission_rate(&usdt_id()).deposit, Some(100));
        treasury.set_commission_rate(
            &usdt_id(),
            CommissionRate {
                deposit: Some(1000),
                withdraw: Some(2000),
            },
        );
        assert_eq!(treasury.commission_rate(&usdt_id()).deposit, Some(1000));
        assert_eq!(treasury.commission_rate(&usdt_id()).withdraw, Some(2000));
    }

    #[test]
    fn test_set_zero_commission_rate() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        assert_eq!(treasury.commission_rate(&usdt_id()).deposit, Some(100));
        treasury.set_commission_rate(
            &usdt_id(),
            CommissionRate {
                deposit: Some(0),
                withdraw: Some(0),
            },
        );
        assert_eq!(treasury.commission_rate(&usdt_id()).deposit, Some(0));
        assert_eq!(treasury.commission_rate(&usdt_id()).withdraw, Some(0));
    }

    #[test]
    fn test_set_none_commission_rate() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        assert_eq!(treasury.commission_rate(&usdt_id()).deposit, Some(100));
        treasury.set_commission_rate(
            &usdt_id(),
            CommissionRate {
                deposit: None,
                withdraw: None,
            },
        );
        assert_eq!(
            treasury.commission_rate(&usdt_id()).deposit,
            Some(INITIAL_COMMISSION_RATE)
        );
        assert_eq!(
            treasury.commission_rate(&usdt_id()).withdraw,
            Some(INITIAL_COMMISSION_RATE)
        );
    }

    #[test]
    #[should_panic(expected = "Commission rate cannot be more than 5%")]
    fn test_set_exceeded_deposit_commission_rate() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        assert_eq!(treasury.commission_rate(&usdt_id()).deposit, Some(100));
        treasury.set_commission_rate(
            &usdt_id(),
            CommissionRate {
                deposit: Some(MAX_COMMISSION_RATE + 1),
                withdraw: None,
            },
        );
    }

    #[test]
    #[should_panic(expected = "Commission rate cannot be more than 5%")]
    fn test_set_exceeded_withdraw_commission_rate() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        assert_eq!(treasury.commission_rate(&usdt_id()).withdraw, Some(100));
        treasury.set_commission_rate(
            &usdt_id(),
            CommissionRate {
                deposit: None,
                withdraw: Some(MAX_COMMISSION_RATE + 1),
            },
        );
    }

    #[test]
    fn test_view_commission_rate() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        assert_eq!(treasury.commission_rate(&usdt_id()).deposit, Some(100));
        assert_eq!(treasury.commission_rate(&usdt_id()).withdraw, Some(100));
        treasury.set_commission_rate(
            &usdt_id(),
            CommissionRate {
                deposit: Some(1000),
                withdraw: Some(5000),
            },
        );
        assert_eq!(treasury.commission_rate(&usdt_id()).deposit.unwrap(), 1000);
        assert_eq!(treasury.commission_rate(&usdt_id()).withdraw.unwrap(), 5000);
    }

    #[test]
    #[should_panic(expected = "Asset bob is not supported")]
    fn test_view_not_existed_asset_commission_rate() {
        let treasury = StableTreasury::new(StorageKey::StableTreasury);
        treasury.commission_rate(&accounts(1));
    }
}
