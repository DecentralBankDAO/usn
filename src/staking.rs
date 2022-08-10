use crate::*;

const GAS_SURPLUS: Gas = Gas(7_000_000_000_000);
const GAS_FOR_GET_ACCOUNT: Gas = Gas(7_000_000_000_000);
const GAS_FOR_STAKE: Gas = Gas(35_000_000_000_000);
const GAS_FOR_UNSTAKE: Gas = Gas(35_000_000_000_000);
const GAS_FOR_WITHDRAW: Gas = Gas(35_000_000_000_000);

#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct HumanReadableAccount {
    pub account_id: AccountId,
    /// The unstaked balance that can be withdrawn or staked.
    pub unstaked_balance: U128,
    /// The amount balance staked at the current "stake" share price.
    pub staked_balance: U128,
    /// Whether the unstaked balance is available for withdrawal now.
    pub can_withdraw: bool,
}

#[ext_contract(ext_pool)]
pub trait StackingPool {
    fn deposit_and_stake(&mut self);

    fn unstake(&mut self, amount: U128);

    fn unstake_all(&mut self);

    fn withdraw_all(&mut self);

    fn get_account(&self, account_id: AccountId) -> HumanReadableAccount;
}

pub fn stake(amount: U128, pool_id: AccountId) -> Promise {
    assert!(
        amount.0 <= env::account_balance(),
        "The account doesn't have enough balance"
    );

    ext_pool::deposit_and_stake(pool_id, amount.0, GAS_FOR_STAKE)
}

pub fn withdraw_all(pool_id: AccountId) -> Promise {
    ext_pool::withdraw_all(pool_id, NO_DEPOSIT, GAS_FOR_WITHDRAW)
}

pub fn unstake(amount: U128, pool_id: AccountId) -> Promise {
    ext_pool::get_account(
        env::current_account_id(),
        pool_id.clone(),
        NO_DEPOSIT,
        GAS_FOR_GET_ACCOUNT,
    )
    .then(ext_self::handle_unstake(
        amount,
        pool_id,
        env::current_account_id(),
        NO_DEPOSIT,
        GAS_SURPLUS + GAS_FOR_UNSTAKE,
    ))
}

pub fn unstake_all(pool_id: AccountId) -> Promise {
    ext_pool::unstake_all(pool_id, NO_DEPOSIT, GAS_FOR_UNSTAKE)
}

#[ext_contract(ext_self)]
trait SelfHandler {
    #[private]
    fn handle_unstake(
        &mut self,
        amount: U128,
        pool_id: AccountId,
        #[callback] account_info: HumanReadableAccount,
    ) -> Promise;
}

trait SelfHandler {
    fn handle_unstake(
        &mut self,
        amount: U128,
        pool_id: AccountId,
        account_info: HumanReadableAccount,
    ) -> Promise;
}

#[near_bindgen]
impl SelfHandler for Contract {
    #[private]
    fn handle_unstake(
        &mut self,
        amount: U128,
        pool_id: AccountId,
        #[callback] account_info: HumanReadableAccount,
    ) -> Promise {
        let unstake_amount = if amount.0 <= account_info.staked_balance.0 {
            amount.0
        } else {
            account_info.staked_balance.0
        };
        ext_pool::unstake(unstake_amount.into(), pool_id, NO_DEPOSIT, GAS_FOR_UNSTAKE)
    }
}
