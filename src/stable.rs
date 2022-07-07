use crate::*;

use near_sdk::{collections::UnorderedMap, IntoStorageKey};

const USDT_DECIMALS: u8 = 6;
const COMMISSION_INTEREST: u128 = 100; // 0.0001 = 0.01%

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

#[derive(BorshDeserialize, BorshSerialize, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub enum StableStatus {
    Active,
    Disabled,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct StableInfo {
    decimals: u8,
    commission: U128,
    status: StableStatus,
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct StableTreasury {
    stable_token: UnorderedMap<AccountId, StableInfo>,
}

impl StableTreasury {
    pub fn new<S>(prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        let mut this = Self {
            stable_token: UnorderedMap::new(prefix),
        };

        // USDT is supported by default.
        this.add_token(&usdt_id(), USDT_DECIMALS);
        this
    }

    pub fn add_token(&mut self, token_id: &AccountId, decimals: u8) {
        assert!(self.stable_token.get(&token_id).is_none());
        let token_info = StableInfo {
            decimals,
            commission: U128(0),
            status: StableStatus::Active,
        };
        self.stable_token.insert(token_id, &token_info);
    }

    pub fn enable_token(&mut self, token_id: &AccountId) {
        self.assert_asset(&token_id);
        self.assert_status(&token_id, StableStatus::Disabled);
        let mut token_info = self.stable_token.get(token_id).unwrap();
        token_info.status = StableStatus::Active;
        self.stable_token.insert(token_id, &token_info);
    }

    pub fn disable_token(&mut self, token_id: &AccountId) {
        self.assert_asset(&token_id);
        self.assert_status(&token_id, StableStatus::Active);
        let mut token_info = self.stable_token.get(token_id).unwrap();
        token_info.status = StableStatus::Disabled;
        self.stable_token.insert(token_id, &token_info);
    }

    pub fn supported_tokens(&self) -> Vec<(AccountId, StableInfo)> {
        self.stable_token.to_vec()
    }

    pub fn deposit(
        &mut self,
        ft: &mut FungibleTokenFreeStorage,
        account_id: &AccountId,
        token_id: &AccountId,
        token_amount: Balance,
    ) {
        self.assert_asset(token_id);
        self.assert_status(&token_id, StableStatus::Active);
        let token = self.stable_token.get(token_id).unwrap();
        let amount = self.convert_decimals(token_amount, token.decimals, USN_DECIMALS);
        let amount_without_fee = self.withdraw_commission(token_id, amount);
        ft.internal_deposit(account_id, amount_without_fee);
        event::emit::ft_mint(account_id, amount_without_fee, None);
    }

    pub fn withdraw(
        &mut self,
        ft: &mut FungibleTokenFreeStorage,
        account_id: &AccountId,
        token_id: &AccountId,
        amount: Balance,
    ) -> u128 {
        self.assert_asset(&token_id);
        self.assert_status(&token_id, StableStatus::Active);
        let token = self.stable_token.get(token_id).unwrap();
        let amount_without_fee = self.withdraw_commission(token_id, amount);
        let token_amount = self.convert_decimals(amount_without_fee, USN_DECIMALS, token.decimals);
        assert_ne!(
            token_amount, 0,
            "Not enough USN: specified amount exchanges to 0 tokens"
        );
        ft.internal_withdraw(account_id, amount);
        event::emit::ft_burn(account_id, amount, None);
        token_amount
    }

    pub fn refund(
        &mut self,
        ft: &mut FungibleTokenFreeStorage,
        account_id: &AccountId,
        token_id: &AccountId,
        original_amount: Balance,
    ) {
        self.assert_asset(&token_id);
        self.assert_status(&token_id, StableStatus::Active);
        self.refund_commission(token_id, original_amount);
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

    fn assert_asset(&self, token_id: &AccountId) {
        if !self.stable_token.get(token_id).is_some() {
            env::panic_str(&format!("Asset {} is not supported", token_id));
        }
    }

    fn assert_status(&self, token_id: &AccountId, status: StableStatus) {
        if self.stable_token.get(token_id).unwrap().status != status {
            env::panic_str(&format!("Asset {} is currently not {:?}", token_id, status));
        }
    }

    fn withdraw_commission(&mut self, token_id: &AccountId, amount: u128) -> u128 {
        let mut token_info = self.stable_token.get(token_id).unwrap();

        let spread_denominator = 10u128.pow(SPREAD_DECIMAL as u32);
        let commission = amount * COMMISSION_INTEREST / spread_denominator; // amount * 0.0001
        token_info.commission = (token_info.commission.0 + commission).into();
        self.stable_token.insert(token_id, &token_info);

        amount - commission
    }

    pub fn refund_commission(&mut self, token_id: &AccountId, amount: u128) {
        let mut token_info = self.stable_token.get(token_id).unwrap();
        let spread_denominator = 10u128.pow(SPREAD_DECIMAL as u32);
        let commission = amount * COMMISSION_INTEREST / spread_denominator; // amount * 0.0001
        token_info.commission = (token_info.commission.0 - commission).into();
        self.stable_token.insert(token_id, &token_info);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use near_sdk::test_utils::accounts;

    #[test]
    fn test_stable_assets() {
        let treasury = StableTreasury::new(StorageKey::StableTreasury);
        assert_eq!(treasury.supported_tokens()[0].0, usdt_id());
        assert_eq!(treasury.supported_tokens()[0].1.decimals, 6);
    }

    #[test]
    fn test_stable_tokens() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        treasury.add_token(&accounts(1), 20);
        assert!(treasury.stable_token.get(&accounts(1)).is_some());
    }

    #[test]
    fn test_enable_disable_tokens() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        treasury.add_token(&accounts(1), 20);
        assert_eq!(
            treasury.supported_tokens()[1].1.status,
            StableStatus::Active
        );
        treasury.disable_token(&accounts(1));
        assert_eq!(
            treasury.supported_tokens()[1].1.status,
            StableStatus::Disabled
        );
        treasury.enable_token(&accounts(1));
        assert_eq!(
            treasury.supported_tokens()[1].1.status,
            StableStatus::Active
        );
    }

    #[test]
    #[should_panic]
    fn test_add_tokens() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        treasury.add_token(&accounts(1), 20);
        treasury.add_token(&accounts(1), 20);
    }

    #[test]
    fn test_view_stable_tokens() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        treasury.add_token(&accounts(1), 20);
        assert_eq!(treasury.supported_tokens().len(), 2);
        assert_eq!(treasury.supported_tokens()[1].0, accounts(1));
    }

    #[test]
    fn test_convert_decimals() {
        let treasury = StableTreasury::new(StorageKey::StableTreasury);
        let amount = 10000;
        let token_decimals = 20;
        let usn_amount = treasury.convert_decimals(amount, token_decimals, USN_DECIMALS);
        assert_eq!(usn_amount, 100);
        let stable_amount = treasury.convert_decimals(usn_amount, USN_DECIMALS, token_decimals);
        assert_eq!(stable_amount, amount);
    }

    #[test]
    fn test_calculate_commission() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let amount_with_fee = treasury.withdraw_commission(&usdt_id(), 100000);
        assert_eq!(amount_with_fee, 99990);
        assert_eq!(treasury.supported_tokens()[0].1.commission, U128(10));
        treasury.withdraw_commission(&usdt_id(), 100000);
        assert_eq!(treasury.supported_tokens()[0].1.commission, U128(20));
    }

    #[test]
    #[should_panic(expected = "Asset charlie is not supported")]
    fn test_deposit_not_supported_asset() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let mut token = FungibleTokenFreeStorage::new(StorageKey::Token);
        treasury.deposit(&mut token, &accounts(1), &accounts(2), 10000);
    }

    #[test]
    #[should_panic(expected = "Asset usdt.test.near is currently not Active")]
    fn test_deposit_not_active_asset() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let mut token = FungibleTokenFreeStorage::new(StorageKey::Token);
        treasury.disable_token(&usdt_id());
        treasury.deposit(&mut token, &accounts(2), &usdt_id(), 10000);
    }

    #[test]
    fn test_deposit() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let mut token = FungibleTokenFreeStorage::new(StorageKey::Token);

        treasury.add_token(&accounts(2), 20);
        treasury.deposit(&mut token, &accounts(1), &accounts(2), 1000000);
        assert_eq!(token.accounts.get(&accounts(1)).unwrap(), 9999);
    }

    #[test]
    fn test_deposit_withdraw() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let mut token = FungibleTokenFreeStorage::new(StorageKey::Token);

        treasury.add_token(&accounts(2), 8);
        treasury.deposit(&mut token, &accounts(1), &accounts(2), 100000);
        let usn_amount = token.accounts.get(&accounts(1)).unwrap();
        assert_eq!(usn_amount, 999900000000000);

        treasury.withdraw(&mut token, &accounts(1), &accounts(2), usn_amount);
        assert!(token.accounts.get(&accounts(1)).is_none());
    }

    #[test]
    fn test_conversion_loss() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let mut token = FungibleTokenFreeStorage::new(StorageKey::Token);

        treasury.add_token(&accounts(2), 8);
        treasury.deposit(&mut token, &accounts(1), &accounts(2), 100000);
        assert_eq!(token.accounts.get(&accounts(1)).unwrap(), 999900000000000);

        token.internal_withdraw(&accounts(1), 1000);
        let usn_amount = token.accounts.get(&accounts(1)).unwrap();

        let withdrawn = treasury.withdraw(&mut token, &accounts(1), &accounts(2), usn_amount);
        assert_eq!(token.accounts.get(&accounts(1)), None);
        assert_eq!(withdrawn, 99980);
    }

    #[test]
    #[should_panic(expected = "Not enough USN: specified amount exchanges to 0 tokens")]
    fn test_conversion_loss_with_panic() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let mut token = FungibleTokenFreeStorage::new(StorageKey::Token);

        treasury.add_token(&accounts(2), 8);
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

        treasury.add_token(&accounts(2), 8);
        treasury.deposit(&mut token, &accounts(1), &accounts(2), 100000);
        let usn_amount = token.accounts.get(&accounts(1)).unwrap();
        token.internal_withdraw(&accounts(1), 1000);

        treasury.withdraw(&mut token, &accounts(1), &accounts(2), usn_amount);
    }

    #[test]
    #[should_panic]
    fn test_withdraw_less() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let mut token = FungibleTokenFreeStorage::new(StorageKey::Token);

        treasury.add_token(&accounts(2), 8);
        token.internal_deposit(&accounts(1), 100000);
        let usn_amount = token.accounts.get(&accounts(1)).unwrap();
        treasury.withdraw(&mut token, &accounts(1), &accounts(2), usn_amount);
    }

    #[test]
    fn test_refund_commission() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        treasury.add_token(&accounts(2), 6);
        treasury.withdraw_commission(&accounts(2), 10000000000000000000000000000000);
        assert_eq!(
            treasury
                .stable_token
                .get(&accounts(2))
                .unwrap()
                .commission
                .0,
            1000000000000000000000000000
        );
        treasury.refund_commission(&accounts(2), 10000000000000000000000000000000);
        assert_eq!(
            treasury
                .stable_token
                .get(&accounts(2))
                .unwrap()
                .commission
                .0,
            0
        );
    }
}
