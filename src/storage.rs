use crate::*;

use near_contract_standards::storage_management::{
    StorageBalance, StorageBalanceBounds, StorageManagement,
};

#[near_bindgen]
impl Contract {
    /// Always returns 125 milliNEAR indicating that user doesn't need to be registered.
    /// It's a workaround for integrations required NEP-125 storage compatibility.
    pub fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        const DEFAULT_STORAGE: u128 = 1250000000000000000000;
        let burrow_storage = self.burrow.storage_balance_of(account_id);
        if let Some(storage) = burrow_storage {
            Some(StorageBalance {
                total: (storage.total.0 + DEFAULT_STORAGE).into(),
                available: storage.available,
            })
        } else {
            Some(StorageBalance {
                total: DEFAULT_STORAGE.into(),
                available: 0.into(),
            })
        }
    }

    /// Doesn't need to be called in case of simple USN usage
    /// Call in order to use Burrow functionality
    #[payable]
    pub fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        self.burrow.storage_deposit(account_id, registration_only)
    }

    #[payable]
    pub fn storage_withdraw(&mut self, amount: Option<U128>) -> StorageBalance {
        assert_one_yocto();
        self.burrow.storage_withdraw(amount)
    }

    #[payable]
    pub fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        self.burrow.storage_unregister(force)
    }

    pub fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        self.burrow.storage_balance_bounds()
    }

    /// Helper method for debugging storage usage that ignores minimum storage limits.
    pub fn debug_storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        self.burrow.debug_storage_balance_of(account_id)
    }
}
