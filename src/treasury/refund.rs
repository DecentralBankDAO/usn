use super::balance_treasury::CONFIG;
use super::ft::ext_ft;
use super::gas::*;
use super::pool::Pool;
use super::ref_finance::*;
use crate::*;
use near_sdk::{require, ONE_YOCTO};
use std::collections::HashMap;

#[near_bindgen]
impl Contract {
    #[payable]
    pub fn refund(&mut self) -> PromiseOrValue<()> {
        self.assert_owner_or_guardian();

        require!(
            env::attached_deposit() == 3 * ONE_YOCTO,
            "3 yoctoNEAR of attached deposit is required"
        );

        let pool = Pool::pool();

        ext_ref_finance::get_deposits(
            env::current_account_id(),
            pool.ref_id,
            NO_DEPOSIT,
            GAS_FOR_GET_DEPOSITS,
        )
        .then(ext_self::handle_refund_to_pool(
            env::current_account_id(),
            env::attached_deposit(),
            GAS_SURPLUS * 5
                + GAS_FOR_WITHDRAW
                + GAS_FOR_NEAR_WITHDRAW
                + GAS_FOR_ADD_LIQUIDITY
                + GAS_FOR_GET_BALANCE,
        ))
        .into()
    }
}

#[ext_contract(ext_self)]
trait SelfHandler {
    #[private]
    #[payable]
    fn handle_refund_to_pool(
        &mut self,
        #[callback] deposits: HashMap<AccountId, U128>,
    ) -> PromiseOrValue<()>;

    #[private]
    #[payable]
    fn handle_refund_from_wnear(
        &mut self,
        wnear_deposit: Balance,
        #[callback] wnear_balance: U128,
    ) -> PromiseOrValue<()>;
}

trait SelfHandler {
    fn handle_refund_to_pool(&mut self, deposits: HashMap<AccountId, U128>) -> PromiseOrValue<()>;

    fn handle_refund_from_wnear(
        &mut self,
        wnear_deposit: Balance,
        wnear_balance: U128,
    ) -> PromiseOrValue<()>;
}

#[near_bindgen]
impl SelfHandler for Contract {
    #[private]
    #[payable]
    fn handle_refund_to_pool(
        &mut self,
        #[callback] deposits: HashMap<AccountId, U128>,
    ) -> PromiseOrValue<()> {
        let pool = Pool::pool();

        let wrap_id: AccountId = CONFIG.wrap_id.parse().unwrap();

        let usdt_deposit = deposits
            .get(&pool.tokens[1])
            .unwrap_or(&U128::from(0u128))
            .0;
        let wnear_deposit = deposits.get(&wrap_id).unwrap_or(&U128::from(0u128)).0;

        let get_wnear_balance = ext_ft::ft_balance_of(
            env::current_account_id(),
            wrap_id,
            NO_DEPOSIT,
            GAS_FOR_GET_BALANCE,
        );
        let refund_wnear = ext_self::handle_refund_from_wnear(
            wnear_deposit,
            env::current_account_id(),
            2 * ONE_YOCTO,
            GAS_SURPLUS * 2 + GAS_FOR_WITHDRAW + GAS_FOR_NEAR_WITHDRAW,
        );

        if usdt_deposit > 0 {
            let mut add_amounts: Vec<U128> = Vec::new();
            add_amounts.push(U128(0));
            add_amounts.push(U128(usdt_deposit));

            let min_shares = U128::from(0u128);

            ext_ref_finance::add_stable_liquidity(
                pool.id,
                add_amounts,
                min_shares,
                pool.ref_id.clone(),
                ONE_YOCTO,
                GAS_FOR_ADD_LIQUIDITY,
            )
            .then(get_wnear_balance)
            .then(refund_wnear)
            .into()
        } else {
            get_wnear_balance.then(refund_wnear).into()
        }
    }

    #[private]
    #[payable]
    fn handle_refund_from_wnear(
        &mut self,
        wnear_deposit: Balance,
        #[callback] wnear_balance: U128,
    ) -> PromiseOrValue<()> {
        let pool = Pool::pool();
        let wrap_id: AccountId = CONFIG.wrap_id.parse().unwrap();

        let withdraw_wnear = ext_ft::near_withdraw(
            U128(wnear_balance.0 + wnear_deposit),
            wrap_id.clone(),
            ONE_YOCTO,
            GAS_FOR_NEAR_WITHDRAW,
        );

        if wnear_deposit > 0 {
            ext_ref_finance::withdraw(
                wrap_id,
                U128(wnear_deposit),
                None,
                pool.ref_id,
                ONE_YOCTO,
                GAS_FOR_WITHDRAW,
            )
            .then(withdraw_wnear)
            .into()
        } else if wnear_balance.0 > 0 {
            withdraw_wnear.into()
        } else {
            PromiseOrValue::Value(())
        }
    }
}
