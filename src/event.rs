pub mod emit {
    use near_contract_standards::fungible_token::events::{FtBurn, FtMint};

    use crate::*;

    pub fn ft_mint(owner_id: &AccountId, amount: Balance, memo: Option<&str>) {
        (FtMint {
            owner_id: owner_id,
            amount: &amount.into(),
            memo: memo,
        })
        .emit();
    }

    pub fn ft_burn(owner_id: &AccountId, amount: Balance, memo: Option<&str>) {
        (FtBurn {
            owner_id: owner_id,
            amount: &amount.into(),
            memo: memo,
        })
        .emit();
    }
}
