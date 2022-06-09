use crate::*;

use super::gas::*;
use super::pool::Pool;
use super::ref_finance::*;

use near_sdk::{require, ONE_YOCTO};

#[near_bindgen]
impl Contract {
    #[payable]
    /// Removes stable liquidity from ref.finance on behalf of "usn".
    /// Burns necessary amount of USN.
    ///
    /// It effectively reverts `transfer_stable_liquidity` action fixating
    /// collateralization on the static accounts of USDT and USN contracts
    ///
    /// It fails if 'usn' is the only liquidity provider in the stable pool.
    pub fn exit_stable_pool(&mut self) -> Promise {
        self.assert_owner();

        let pool = Pool::stable_pool();

        // 3 yoctoNEARs: 2 `withdraw` and 1 `remove_liquidity`.
        require!(
            env::attached_deposit() == 3 * ONE_YOCTO,
            "Requires exactly 3 yoctoNEAR of attached deposit"
        );

        ext_ref_finance::get_pool_shares(
            pool.id,
            env::current_account_id(),
            pool.ref_id.clone(),
            NO_DEPOSIT,
            GAS_FOR_GET_SHARES,
        )
        .then(ext_self::handle_start_removing(
            env::current_account_id(),
            env::attached_deposit(),
            GAS_SURPLUS * 6
                + GAS_FOR_REMOVE_LIQUIDITY
                + GAS_FOR_WITHDRAW * 2
                + GAS_FOR_FINISH_BURNING,
        ))
    }
}

#[ext_contract(ext_self)]
trait RefFinanceHandler {
    #[private]
    #[payable]
    fn handle_start_removing(&mut self, #[callback] shares: U128) -> Promise;

    #[private]
    #[payable]
    fn handle_remove_deposit(&mut self, #[callback] amounts: Vec<U128>) -> Promise;

    #[private]
    fn finish_removing_with_burn(&mut self, amount: U128);
}

trait RefFinanceHandler {
    fn handle_start_removing(&mut self, shares: U128) -> Promise;

    fn handle_remove_deposit(&mut self, amounts: Vec<U128>) -> Promise;

    fn finish_removing_with_burn(&mut self, amount: U128);
}

#[near_bindgen]
impl RefFinanceHandler for Contract {
    #[private]
    #[payable]
    fn handle_start_removing(&mut self, #[callback] shares: U128) -> Promise {
        let pool = Pool::stable_pool();
        let min_amounts = vec![U128(0), U128(0)];

        // 3 yoctoNEARs: 2 `withdraw` and 1 `remove_liquidity`.
        require!(
            env::attached_deposit() == 3 * ONE_YOCTO,
            "Requires exactly 3 yoctoNEAR of attached deposit"
        );

        ext_ref_finance::remove_liquidity(
            pool.id,
            shares,
            min_amounts,
            pool.ref_id,
            ONE_YOCTO,
            GAS_FOR_REMOVE_LIQUIDITY,
        )
        .then(ext_self::handle_remove_deposit(
            env::current_account_id(),
            ONE_YOCTO * 2,
            GAS_SURPLUS * 3 + GAS_FOR_WITHDRAW * 2 + GAS_FOR_FINISH_BURNING,
        ))
    }

    // Removes deposits from ref.finances (getting them back to token accounts).
    // This is an oversimplified method which works with just one predefined pool,
    // and the first token is USN.
    #[private]
    #[payable]
    fn handle_remove_deposit(&mut self, #[callback] amounts: Vec<U128>) -> Promise {
        let pool = Pool::stable_pool();

        require!(amounts.len() == 2);
        require!(pool.tokens[0] == env::current_account_id());

        ext_ref_finance::withdraw(
            pool.tokens[1].clone(),
            amounts[1],
            Some(false),
            pool.ref_id.clone(),
            ONE_YOCTO,
            GAS_FOR_WITHDRAW,
        )
        .then(ext_ref_finance::withdraw(
            pool.tokens[0].clone(),
            amounts[0],
            Some(false),
            pool.ref_id,
            ONE_YOCTO,
            GAS_FOR_WITHDRAW,
        ))
        .then(ext_self::finish_removing_with_burn(
            amounts[0],
            env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_FINISH_BURNING,
        ))
    }

    #[private]
    fn finish_removing_with_burn(&mut self, amount: U128) {
        if is_promise_success() {
            self.token
                .internal_withdraw(&env::current_account_id(), amount.into());
            event::emit::ft_burn(&env::current_account_id(), amount.into(), None);
        }
    }
}
