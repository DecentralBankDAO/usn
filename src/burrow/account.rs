use super::*;
use crate::*;

#[derive(BorshSerialize, BorshDeserialize, Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Account {
    /// A copy of an account ID. Saves one storage_read when iterating on accounts.
    pub account_id: AccountId,
    /// A list of assets that are supplied by the account (but not used a collateral).
    /// It's not returned for account pagination.
    pub supplied: HashMap<TokenId, Shares>,
    /// A list of collateral assets.
    pub collateral: HashMap<TokenId, Shares>,
    /// A list of borrowed assets.
    pub borrowed: HashMap<TokenId, Shares>,
    /// Keeping track of data required for farms for this account.
    #[serde(skip_serializing)]
    pub farms: HashMap<FarmId, AccountFarm>,
    #[borsh_skip]
    #[serde(skip_serializing)]
    pub affected_farms: HashSet<FarmId>,

    /// Tracks changes in storage usage by persistent collections in this account.
    #[borsh_skip]
    #[serde(skip)]
    pub storage_tracker: StorageTracker,

    /// Staking of booster token.
    pub booster_staking: Option<BoosterStaking>,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub enum VAccount {
    Current(Account),
}

impl VAccount {
    pub fn into_account(self) -> Account {
        match self {
            VAccount::Current(c) => c,
        }
    }
}

impl From<Account> for VAccount {
    fn from(c: Account) -> Self {
        VAccount::Current(c)
    }
}

impl Account {
    pub fn new(account_id: &AccountId) -> Self {
        Self {
            account_id: account_id.clone(),
            supplied: HashMap::new(),
            collateral: HashMap::new(),
            borrowed: HashMap::new(),
            farms: HashMap::new(),
            affected_farms: HashSet::new(),
            storage_tracker: Default::default(),
            booster_staking: None,
        }
    }

    pub fn increase_collateral(&mut self, token_id: &TokenId, shares: Shares) {
        self.collateral
            .entry(token_id.clone())
            .or_insert_with(|| 0.into())
            .0 += shares.0;
    }

    pub fn decrease_collateral(&mut self, token_id: &TokenId, shares: Shares) {
        let current_collateral = self.internal_unwrap_collateral(token_id);
        if let Some(new_balance) = current_collateral.0.checked_sub(shares.0) {
            if new_balance > 0 {
                self.collateral
                    .insert(token_id.clone(), Shares::from(new_balance));
            } else {
                self.collateral.remove(token_id);
            }
        } else {
            env::panic_str("Not enough collateral balance");
        }
    }

    pub fn increase_borrowed(&mut self, token_id: &TokenId, shares: Shares) {
        self.borrowed
            .entry(token_id.clone())
            .or_insert_with(|| 0.into())
            .0 += shares.0;
    }

    pub fn decrease_borrowed(&mut self, token_id: &TokenId, shares: Shares) {
        let current_borrowed = self.internal_unwrap_borrowed(token_id);
        if let Some(new_balance) = current_borrowed.0.checked_sub(shares.0) {
            if new_balance > 0 {
                self.borrowed
                    .insert(token_id.clone(), Shares::from(new_balance));
            } else {
                self.borrowed.remove(token_id);
            }
        } else {
            env::panic_str("Not enough borrowed balance");
        }
    }

    pub fn internal_unwrap_collateral(&mut self, token_id: &TokenId) -> Shares {
        *self
            .collateral
            .get(&token_id)
            .expect("Collateral asset not found")
    }

    pub fn internal_unwrap_borrowed(&mut self, token_id: &TokenId) -> Shares {
        *self
            .borrowed
            .get(&token_id)
            .expect("Borrowed asset not found")
    }

    pub fn add_affected_farm(&mut self, farm_id: FarmId) -> bool {
        self.affected_farms.insert(farm_id)
    }

    /// Returns all assets that can be potentially farmed.
    pub fn get_all_potential_farms(&self) -> HashSet<FarmId> {
        let mut potential_farms = HashSet::new();
        potential_farms.insert(FarmId::NetTvl);
        potential_farms.extend(self.supplied.keys().cloned().map(FarmId::Supplied));
        potential_farms.extend(self.collateral.keys().cloned().map(FarmId::Supplied));
        potential_farms.extend(self.borrowed.keys().cloned().map(FarmId::Borrowed));
        potential_farms
    }

    pub fn get_supplied_shares(&self, token_id: &TokenId) -> Shares {
        let collateral_shares = self.collateral.get(&token_id).map(|s| s.0).unwrap_or(0);
        let supplied_shares = self
            .internal_get_asset(token_id)
            .map(|asset| asset.shares.0)
            .unwrap_or(0);
        (supplied_shares + collateral_shares).into()
    }

    pub fn get_borrowed_shares(&self, token_id: &TokenId) -> Shares {
        self.borrowed
            .get(&token_id)
            .cloned()
            .unwrap_or_else(|| 0.into())
    }
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct CollateralAsset {
    pub token_id: TokenId,
    pub shares: Shares,
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct BorrowedAsset {
    pub token_id: TokenId,
    pub shares: Shares,
}

impl Burrow {
    pub fn internal_get_account(&self, account_id: &AccountId) -> Option<Account> {
        self.accounts.get(account_id).map(|o| o.into_account())
    }

    pub fn internal_unwrap_account(&self, account_id: &AccountId) -> Account {
        self.internal_get_account(account_id)
            .expect("Account is not registered")
    }

    pub fn internal_set_account(&mut self, account_id: &AccountId, mut account: Account) {
        let mut storage = self.internal_unwrap_storage(account_id);
        storage
            .storage_tracker
            .consume(&mut account.storage_tracker);
        storage.storage_tracker.start();
        self.accounts.insert(account_id, &account.into());
        storage.storage_tracker.stop();
        self.internal_set_storage(account_id, storage);
    }
}

impl Burrow {
    pub fn get_account(&self, account_id: AccountId) -> Option<AccountDetailedView> {
        self.internal_get_account(&account_id)
            .map(|account| self.account_into_detailed_view(account))
    }

    pub fn get_accounts_paged(&self, from_index: Option<u64>, limit: Option<u64>) -> Vec<Account> {
        let values = self.accounts.values_as_vector();
        let from_index = from_index.unwrap_or(0);
        let limit = limit.unwrap_or(values.len());
        (from_index..std::cmp::min(values.len(), from_index + limit))
            .map(|index| values.get(index).unwrap().into_account())
            .collect()
    }

    pub fn get_num_accounts(&self) -> u32 {
        self.accounts.len() as _
    }
}
