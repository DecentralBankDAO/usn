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

#[derive(BorshDeserialize, BorshSerialize, PartialEq, Debug, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub enum AssetStatus {
    Enabled,
    Disabled,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct AssetInfo {
    decimals: u8,
    // Stored in USN due to more precise value
    commission: U128,
    status: AssetStatus,
}

impl AssetInfo {
    pub fn new(decimals: u8) -> Self {
        assert!(
            decimals > 0 && decimals <= MAX_VALID_DECIMALS,
            "Decimal value is out of bounds"
        );

        AssetInfo {
            decimals,
            commission: U128(0),
            status: AssetStatus::Enabled,
        }
    }
}

impl From<UnorderedMap<AccountId, AssetInfo>> for StableTreasury {
    fn from(assets: UnorderedMap<AccountId, AssetInfo>) -> Self {
        Self {
            assets,
            commission_rate: INITIAL_COMMISSION_RATE,
        }
    }
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct StableTreasury {
    assets: UnorderedMap<AccountId, AssetInfo>,
    commission_rate: u32,
}

impl StableTreasury {
    pub fn new<S>(prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        let mut this = Self {
            assets: UnorderedMap::new(prefix),
            commission_rate: INITIAL_COMMISSION_RATE,
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
        let amount_without_fee = self.withdraw_commission(asset_id, amount);
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
        let amount_without_fee = self.withdraw_commission(asset_id, amount);
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

    fn withdraw_commission(&mut self, asset_id: &AccountId, amount: u128) -> u128 {
        let mut asset_info = self.assets.get(asset_id).unwrap();
        let commission = amount * self.commission_rate as u128 / 10u128.pow(SPREAD_DECIMAL as u32);
        asset_info.commission = (asset_info.commission.0 + commission).into();
        self.assets.insert(asset_id, &asset_info);

        amount - commission
    }

    fn refund_commission(&mut self, asset_id: &AccountId, amount: u128) {
        let mut asset_info = self.assets.get(asset_id).unwrap();
        let commission = amount * self.commission_rate as u128 / 10u128.pow(SPREAD_DECIMAL as u32);
        asset_info.commission = (asset_info.commission.0 - commission).into();
        self.assets.insert(asset_id, &asset_info);
    }

    pub fn set_commission_rate(&mut self, rate: u32) {
        assert!(
            rate <= MAX_COMMISSION_RATE,
            "Commission rate cannot be more than 5%"
        );
        self.commission_rate = rate;

        env::log_str(&format!(
            "New commission rate was set: {}%",
            rate as f64 * PERCENT_MULTIPLICATOR as f64 / 10f64.powi(SPREAD_DECIMAL as i32)
        ));
    }

    pub fn commission_rate(&self) -> u32 {
        self.commission_rate
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

        treasury.set_commission_rate(MAX_COMMISSION_RATE);
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

        treasury.set_commission_rate(MAX_COMMISSION_RATE);

        treasury.add_asset(&accounts(2), 8);
        treasury.deposit(&mut token, &accounts(1), &accounts(2), 100000);
        let usn_amount = token.accounts.get(&accounts(1)).unwrap();
        assert_eq!(usn_amount, 950000000000000);
        assert_eq!(
            treasury.supported_assets()[1].1.commission,
            U128(50000000000000)
        );

        let withdrawn = treasury.withdraw(&mut token, &accounts(1), &accounts(2), usn_amount);
        assert_eq!(
            treasury.supported_assets()[1].1.commission,
            U128(97500000000000)
        );
        assert!(token.accounts.get(&accounts(1)).is_none());
        assert_eq!(withdrawn, 90250);
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
    fn test_withdraw_commission() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let amount_without_fee = treasury.withdraw_commission(&usdt_id(), 100000);
        assert_eq!(amount_without_fee, 99990);
        assert_eq!(treasury.supported_assets()[0].1.commission, U128(10));
        let amount_without_fee = treasury.withdraw_commission(&usdt_id(), amount_without_fee);
        assert_eq!(amount_without_fee, 99981);
        assert_eq!(treasury.supported_assets()[0].1.commission, U128(19));
    }

    #[test]
    fn test_refund_commission() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        treasury.withdraw_commission(&usdt_id(), 10000000000000000000000000000000);
        assert_eq!(
            treasury.supported_assets()[0].1.commission.0,
            1000000000000000000000000000
        );
        treasury.refund_commission(&usdt_id(), 10000000000000000000000000000000);
        assert_eq!(treasury.supported_assets()[0].1.commission.0, 0);
    }

    #[test]
    fn test_set_commission_rate() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        assert_eq!(treasury.commission_rate, 100);
        treasury.set_commission_rate(1000);
        assert_eq!(treasury.commission_rate, 1000);
    }

    #[test]
    fn test_set_zero_commission_rate() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        assert_eq!(treasury.commission_rate, 100);
        treasury.set_commission_rate(0);
        assert_eq!(treasury.commission_rate, 0);
    }

    #[test]
    #[should_panic(expected = "Commission rate cannot be more than 5%")]
    fn test_set_exceeded_commission_rate() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        assert_eq!(treasury.commission_rate, 100);
        treasury.set_commission_rate(MAX_COMMISSION_RATE + 1);
    }

    #[test]
    fn test_view_commission_rate() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        assert_eq!(treasury.commission_rate(), 100);
        treasury.set_commission_rate(1000);
        assert_eq!(treasury.commission_rate(), 1000);
    }
}
