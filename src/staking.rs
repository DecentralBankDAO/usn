use crate::*;

const GAS_SURPLUS: Gas = Gas(7_000_000_000_000);
const GAS_FOR_GET_ACCOUNT: Gas = Gas(7_000_000_000_000);
const GAS_FOR_STAKE: Gas = Gas(35_000_000_000_000);
const GAS_FOR_UNSTAKE: Gas = Gas(35_000_000_000_000);
const GAS_FOR_WITHDRAW: Gas = Gas(35_000_000_000_000);

const CONFIG: &'static str = if cfg!(feature = "mainnet") {
    "nearua.poolv1.near"
} else if cfg!(feature = "testnet") {
    "prophet.pool.f863973.m0"
} else {
    "pool.test.near"
};

fn pool() -> AccountId {
    CONFIG.parse().unwrap()
}

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

pub fn stake(amount: U128) -> Promise {
    assert!(
        amount.0 <= env::account_balance(),
        "The account doesn't have enough balance"
    );

    ext_pool::deposit_and_stake(pool(), amount.0, GAS_FOR_STAKE)
}

pub fn withdraw_all() -> Promise {
    ext_pool::withdraw_all(pool(), NO_DEPOSIT, GAS_FOR_WITHDRAW)
}

pub fn unstake(amount: U128) -> Promise {
    ext_pool::get_account(
        env::current_account_id(),
        pool(),
        NO_DEPOSIT,
        GAS_FOR_GET_ACCOUNT,
    )
    .then(ext_self::handle_unstake(
        amount,
        env::current_account_id(),
        NO_DEPOSIT,
        GAS_SURPLUS + GAS_FOR_UNSTAKE,
    ))
}

pub fn unstake_all() -> Promise {
    ext_pool::unstake_all(pool(), NO_DEPOSIT, GAS_FOR_UNSTAKE)
}

#[ext_contract(ext_self)]
trait SelfHandler {
    #[private]
    fn handle_unstake(
        &mut self,
        amount: U128,
        #[callback] account_info: HumanReadableAccount,
    ) -> Promise;
}

trait SelfHandler {
    fn handle_unstake(&mut self, amount: U128, account_info: HumanReadableAccount) -> Promise;
}

#[near_bindgen]
impl SelfHandler for Contract {
    #[private]
    fn handle_unstake(
        &mut self,
        amount: U128,
        #[callback] account_info: HumanReadableAccount,
    ) -> Promise {
        let unstake_amount = if amount.0 <= account_info.staked_balance.0 {
            amount.0
        } else {
            account_info.staked_balance.0
        };
        ext_pool::unstake(unstake_amount.into(), pool(), NO_DEPOSIT, GAS_FOR_UNSTAKE)
    }
}
