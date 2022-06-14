use crate::*;

/// Single swap action.
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct SwapAction {
    /// Pool which should be used for swapping.
    pub pool_id: u64,
    /// Token to swap from.
    pub token_in: AccountId,
    /// Amount to exchange.
    /// If amount_in is None, it will take amount_out from previous step.
    /// Will fail if amount_in is None on the first step.
    pub amount_in: Option<U128>,
    /// Token to swap into.
    pub token_out: AccountId,
    /// Required minimum amount of token_out.
    pub min_amount_out: U128,
}

#[ext_contract(ext_ref_finance)]
trait RefFinance {
    fn get_deposits(&self, account_id: AccountId) -> HashMap<AccountId, U128>;

    fn get_pool_shares(&self, pool_id: u64, account_id: AccountId) -> U128;

    fn predict_remove_liquidity(&self, pool_id: u64, shares: U128) -> Vec<U128>;

    #[payable]
    fn add_stable_liquidity(&mut self, pool_id: u64, amounts: Vec<U128>, min_shares: U128) -> U128;

    #[payable]
    fn remove_liquidity(&mut self, pool_id: u64, shares: U128, min_amounts: Vec<U128>)
        -> Vec<U128>;

    #[payable]
    fn remove_liquidity_by_tokens(
        &mut self,
        pool_id: u64,
        amounts: Vec<U128>,
        max_burn_shares: U128,
    ) -> U128;

    #[payable]
    fn withdraw(&mut self, token_id: AccountId, amount: U128, unregister: Option<bool>);

    #[payable]
    fn swap(&mut self, actions: Vec<SwapAction>, referral_id: Option<AccountId>) -> U128;
}
