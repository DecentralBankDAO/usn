use crate::*;

use near_contract_standards::storage_management::StorageBalance;

#[near_bindgen]
impl Contract {
    /// Always returns 125 milliNEAR indicating that user doesn't need to be registered.
    /// It's a workaround for integrations required NEP-125 storage compatibility.
    pub fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        let _ = account_id;
        Some(StorageBalance {
            total: 1250000000000000000000.into(),
            available: 0.into(),
        })
    }
}
