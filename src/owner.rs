use crate::*;

#[near_bindgen]
impl Contract {
    pub(crate) fn assert_owner(&self) {
        if env::predecessor_account_id() != self.owner_id {
            env::panic_str("This method can be called only by owner")
        }
    }

    pub(crate) fn assert_owner_or_guardian(&self) {
        let predecessor_id = env::predecessor_account_id();
        if predecessor_id != self.owner_id && !self.guardians.contains(&predecessor_id) {
            env::panic_str("This method can be called only by owner or guardian")
        }
    }

    pub fn set_owner(&mut self, owner_id: AccountId) {
        self.assert_owner();
        self.owner_id = owner_id;
    }

    pub fn owner(&self) -> AccountId {
        self.owner_id.clone()
    }

    /// Extend guardians. Only can be called by owner.
    pub fn extend_guardians(&mut self, guardians: Vec<AccountId>) {
        self.assert_owner();
        for guardian in guardians {
            if !self.guardians.insert(&guardian) {
                env::panic_str(&format!("The guardian '{}' already exists", guardian));
            }
        }
    }

    /// Remove guardians. Only can be called by owner.
    pub fn remove_guardians(&mut self, guardians: Vec<AccountId>) {
        self.assert_owner();
        for guardian in guardians {
            if !self.guardians.remove(&guardian) {
                env::panic_str(&format!("The guardian '{}' doesn't exist", guardian));
            }
        }
    }

    pub fn guardians(&self) -> Vec<AccountId> {
        self.guardians.to_vec()
    }
}
