use crate::*;

use near_sdk::{collections::UnorderedMap, IntoStorageKey};

const USDT_DECIMALS: u8 = 6;
const USDC_DECIMALS: u8 = 6;
const DAI_DECIMALS: u8 = 18;
const COMMISSION_INTEREST: u128 = 500; // 0.0005 = 0.05%

pub struct StableConfig {
    pub usdt_id: &'static str,
    pub usdc_id: &'static str,
    pub dai_id: &'static str,
}

pub const CONFIG: StableConfig = if cfg!(feature = "mainnet") {
    StableConfig {
        usdt_id: "dac17f958d2ee523a2206206994597c13d831ec7.factory.bridge.near",
        usdc_id: "a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.factory.bridge.near",
        dai_id: "6b175474e89094c44da98b954eedeac495271d0f.factory.bridge.near",
    }
} else if cfg!(feature = "testnet") {
    StableConfig {
        usdt_id: "usdt.fakes.testnet",
        usdc_id: "usdc.fakes.testnet",
        dai_id: "dai.fakes.testnet",
    }
} else {
    StableConfig {
        usdt_id: "usdt.test.near",
        usdc_id: "usdc.test.near",
        dai_id: "dai.test.near",
    }
};

pub fn usdt_id() -> AccountId {
    CONFIG.usdt_id.parse().unwrap()
}

pub fn usdc_id() -> AccountId {
    CONFIG.usdc_id.parse().unwrap()
}

pub fn dai_id() -> AccountId {
    CONFIG.dai_id.parse().unwrap()
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

        // USDT, USDC, DAI are supported by default.
        this.add_token(&usdt_id(), USDT_DECIMALS);
        this.add_token(&usdc_id(), USDC_DECIMALS);
        this.add_token(&dai_id(), DAI_DECIMALS);
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
        let amount_with_fee = self.withdraw_commission(token_id, amount);
        ft.internal_deposit(account_id, amount_with_fee);
        event::emit::ft_mint(account_id, amount_with_fee, None);
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
        let token_amount = self.convert_decimals(amount, USN_DECIMALS, token.decimals);
        assert_ne!(
            token_amount, 0,
            "Not enough USN: specified amount exchanges to 0 tokens"
        );
        // USN can have higher precision, it means that it won't burn lower decimals.
        let amount = self.convert_decimals(token_amount, token.decimals, USN_DECIMALS);
        ft.internal_withdraw(account_id, amount);
        event::emit::ft_burn(account_id, amount, None);
        self.withdraw_commission(token_id, token_amount)
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
        assert_eq!(treasury.supported_tokens()[1].0, usdc_id());
        assert_eq!(treasury.supported_tokens()[1].1.decimals, 6);
        assert_eq!(treasury.supported_tokens()[2].0, dai_id());
        assert_eq!(treasury.supported_tokens()[2].1.decimals, 18);
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
            treasury.supported_tokens()[3].1.status,
            StableStatus::Active
        );
        treasury.disable_token(&accounts(1));
        assert_eq!(
            treasury.supported_tokens()[3].1.status,
            StableStatus::Disabled
        );
        treasury.enable_token(&accounts(1));
        assert_eq!(
            treasury.supported_tokens()[3].1.status,
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
        assert_eq!(treasury.supported_tokens().len(), 4);
        assert_eq!(treasury.supported_tokens()[3].0, accounts(1));
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
        assert_eq!(amount_with_fee, 99950);
        assert_eq!(treasury.supported_tokens()[0].1.commission, U128(50));
        treasury.withdraw_commission(&usdt_id(), 100000);
        assert_eq!(treasury.supported_tokens()[0].1.commission, U128(100));
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
        assert_eq!(token.accounts.get(&accounts(1)).unwrap(), 9995);
    }

    #[test]
    fn test_deposit_withdraw() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let mut token = FungibleTokenFreeStorage::new(StorageKey::Token);

        treasury.add_token(&accounts(2), 8);
        treasury.deposit(&mut token, &accounts(1), &accounts(2), 100000);
        let usn_amount = token.accounts.get(&accounts(1)).unwrap();
        assert_eq!(usn_amount, 999500000000000);

        treasury.withdraw(&mut token, &accounts(1), &accounts(2), usn_amount);
        assert!(token.accounts.get(&accounts(1)).is_none());
    }

    #[test]
    fn test_convertion_loss() {
        let mut treasury = StableTreasury::new(StorageKey::StableTreasury);
        let mut token = FungibleTokenFreeStorage::new(StorageKey::Token);

        treasury.add_token(&accounts(2), 8);
        treasury.deposit(&mut token, &accounts(1), &accounts(2), 100000);
        assert_eq!(token.accounts.get(&accounts(1)).unwrap(), 999500000000000);

        token.internal_withdraw(&accounts(1), 1000);
        let usn_amount = token.accounts.get(&accounts(1)).unwrap();

        treasury.withdraw(&mut token, &accounts(1), &accounts(2), usn_amount);
        assert_eq!(token.accounts.get(&accounts(1)).unwrap(), 9999999000);
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
}
