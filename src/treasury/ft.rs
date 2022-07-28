use crate::*;

pub const REF_DEPOSIT_ACTION: &'static str = "";

#[ext_contract(ext_ft)]
pub trait Ft {
    #[payable]
    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> Promise;
}
