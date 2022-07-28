use crate::*;

#[ext_contract(ext_ref_finance)]
trait RefFinance {
    fn get_deposits(&self, account_id: AccountId) -> HashMap<AccountId, U128>;

    fn get_pool_shares(&self, pool_id: u64, account_id: AccountId) -> U128;

    #[payable]
    fn add_stable_liquidity(&mut self, pool_id: u64, amounts: Vec<U128>, min_shares: U128) -> U128;

    #[payable]
    fn remove_liquidity(&mut self, pool_id: u64, shares: U128, min_amounts: Vec<U128>)
        -> Vec<U128>;

    #[payable]
    fn withdraw(&mut self, token_id: AccountId, amount: U128, unregister: Option<bool>);
}
