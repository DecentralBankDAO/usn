use super::*;
use crate::*;

#[derive(BorshSerialize, BorshDeserialize)]
pub struct AccountAsset {
    pub shares: Shares,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub enum VAccountAsset {
    Current(AccountAsset),
}

impl From<VAccountAsset> for AccountAsset {
    fn from(v: VAccountAsset) -> Self {
        match v {
            VAccountAsset::Current(c) => c,
        }
    }
}

impl From<AccountAsset> for VAccountAsset {
    fn from(c: AccountAsset) -> Self {
        VAccountAsset::Current(c)
    }
}

impl AccountAsset {
    pub fn new() -> Self {
        Self { shares: 0.into() }
    }

    pub fn deposit_shares(&mut self, shares: Shares) {
        self.shares.0 += shares.0;
    }

    pub fn withdraw_shares(&mut self, shares: Shares) {
        if let Some(new_balance) = self.shares.0.checked_sub(shares.0) {
            self.shares.0 = new_balance;
        } else {
            env::panic_str("Not enough asset balance");
        }
    }

    pub fn is_empty(&self) -> bool {
        self.shares.0 == 0
    }
}

impl Account {
    pub fn internal_unwrap_asset(&self, token_id: &TokenId) -> AccountAsset {
        self.internal_get_asset(token_id).expect("Asset not found")
    }

    pub fn internal_get_asset(&self, token_id: &TokenId) -> Option<AccountAsset> {
        self.supplied
            .get(token_id)
            .map(|&shares| AccountAsset { shares })
    }

    pub fn internal_get_asset_or_default(&mut self, token_id: &TokenId) -> AccountAsset {
        self.internal_get_asset(token_id)
            .unwrap_or_else(AccountAsset::new)
    }

    pub fn internal_set_asset(&mut self, token_id: &TokenId, account_asset: AccountAsset) {
        if account_asset.is_empty() {
            self.supplied.remove(token_id);
        } else {
            self.supplied.insert(token_id.clone(), account_asset.shares);
        }
        self.add_affected_farm(FarmId::Supplied(token_id.clone()));
    }
}
