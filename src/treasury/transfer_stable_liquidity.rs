use std::collections::HashMap;

use crate::*;

use super::ft::*;
use super::gas::*;
use super::pool::Pool;
use super::ref_finance::*;

use near_sdk::{require, ONE_YOCTO};

#[near_bindgen]
impl Contract {
    /// Transfers liquidity of different tokens to ref.finance on behalf of "usn".
    /// Mints necessary amount of USN.
    ///
    ///  * `whole_amount` - token amount WITHOUT decimals, e.g. "1000" means $1000.
    ///
    /// # Algorithm
    ///
    /// Step 1. `TOKEN -> REF`: ft_transfer_call to ref.finance contract
    ///          on behalf of "usn" account.
    /// Step 2. `USN -> REF`: If token transfer successful, then
    ///          * mint as much USN as successfully transferred tokens,
    ///          * internal_transfer_call of minted USN to ref.finance contract.
    /// Step 3. Check balances, ignoring step 1 & 2. It allows to repeat adding liquidity
    ///         next time with full ref.finance deposits (transfers would fail in this case).
    /// Step 4. `REF -> POOL`: add_stable_liquidity to the TOKENS/USN stable pool filling it
    ///         from usn deposit.
    #[payable]
    pub fn transfer_stable_liquidity(&mut self, pool_id: u64, whole_amount: U128) -> Promise {
        self.assert_owner();

        let pool = Pool::from_config_with_assert(pool_id);

        // 1 yoctoNEAR for each ft_transfer_call (except of the internal transfer).
        // More NEARs could be required for add_stable_liquidity().
        let transfer_deposit = pool.tokens.len() as u128 - 1;
        require!(
            env::attached_deposit() > transfer_deposit,
            &format!(
                "Requires attached deposit more than {} yoctoNEAR",
                transfer_deposit
            ),
        );

        require!(
            whole_amount.0 > NO_DEPOSIT,
            "The token amount must be not zero"
        );

        let usn_id = env::current_account_id();

        ext_ref_finance::get_deposits(
            usn_id,
            pool.ref_id.clone(),
            NO_DEPOSIT,
            GAS_FOR_GET_DEPOSITS,
        )
        .then(ext_self::handle_start_transferring(
            pool.id,
            whole_amount,
            env::current_account_id(),
            env::attached_deposit(),
            GAS_FOR_FT_TRANSFER_CALL * pool.tokens.len() as u64
                + GAS_FOR_GET_DEPOSITS
                + GAS_FOR_ADD_LIQUIDITY
                + GAS_SURPLUS * 3,
        ))
    }
}

#[ext_contract(ext_self)]
trait RefFinanceHandler {
    #[private]
    #[payable]
    fn handle_start_transferring(
        &mut self,
        pool_id: u64,
        whole_amount: U128,
        #[callback] deposits: HashMap<AccountId, U128>,
    ) -> Promise;

    #[private]
    #[payable]
    fn handle_deposit_then_add_liquidity(
        &mut self,
        pool_id: u64,
        whole_amount: U128,
        #[callback] deposits: HashMap<AccountId, U128>,
    );
}

trait RefFinanceHandler {
    fn handle_start_transferring(
        &mut self,
        pool_id: u64,
        whole_amount: U128,
        deposits: HashMap<AccountId, U128>,
    ) -> Promise;

    fn handle_deposit_then_add_liquidity(
        &mut self,
        pool_id: u64,
        whole_amount: U128,
        deposits: HashMap<AccountId, U128>,
    );
}

#[near_bindgen]
impl RefFinanceHandler for Contract {
    #[private]
    #[payable]
    fn handle_start_transferring(
        &mut self,
        pool_id: u64,
        whole_amount: U128,
        #[callback] deposits: HashMap<AccountId, U128>,
    ) -> Promise {
        let pool = Pool::from_config_with_assert(pool_id);

        let tokens = pool
            // Convert the whole decimal part to a full number for each token.
            .extend_decimals(whole_amount.into())
            // Find out how much to deposit yet.
            .map(|(token_id, amount)| {
                let deposit = deposits.get(&token_id).unwrap_or(&U128::from(0u128)).0;
                (token_id, amount.saturating_sub(deposit))
            })
            // Keep non-empty values.
            .filter(|&(_, amount)| amount != 0u128);

        // Create many promises for each transfer, like USDT -> ref-finance.
        let maybe_transfers = tokens
            .map(|(token_id, amount)| -> Promise {
                let usn_id = env::current_account_id();
                if token_id != &usn_id {
                    ext_ft::ft_transfer_call(
                        pool.ref_id.clone(),
                        amount.into(),
                        None,
                        REF_DEPOSIT_ACTION.to_string(),
                        token_id.clone(),
                        ONE_YOCTO,
                        GAS_FOR_FT_TRANSFER_CALL,
                    )
                } else {
                    let usn_balance = self.token.internal_unwrap_balance_of(&usn_id);

                    // Mint necessary USN amount.
                    if usn_balance < amount {
                        let yet_to_mint = amount - usn_balance;
                        self.token.internal_deposit(&usn_id, yet_to_mint);
                        event::emit::ft_mint(&usn_id, yet_to_mint, None);
                    }

                    self.token.internal_transfer_call(
                        &usn_id,
                        &pool.ref_id.clone(),
                        amount,
                        GAS_FOR_FT_TRANSFER_CALL,
                        None,
                        REF_DEPOSIT_ACTION.to_string(),
                    )
                }
            })
            // Chain all promises together.
            .reduce(|acc, promise| acc.and(promise));

        let get_deposits = ext_ref_finance::get_deposits(
            env::current_account_id(),
            pool.ref_id.clone(),
            NO_DEPOSIT,
            GAS_FOR_GET_DEPOSITS,
        );

        let add_liquidity = ext_self::handle_deposit_then_add_liquidity(
            pool.id,
            whole_amount,
            env::current_account_id(),
            env::attached_deposit() - ONE_YOCTO * (pool.tokens.len() as u128 - 1),
            GAS_FOR_ADD_LIQUIDITY + GAS_SURPLUS,
        );

        if let Some(transfers) = maybe_transfers {
            transfers.then(get_deposits).then(add_liquidity)
        } else {
            get_deposits.then(add_liquidity)
        }
    }

    #[private]
    #[payable]
    fn handle_deposit_then_add_liquidity(
        &mut self,
        pool_id: u64,
        whole_amount: U128,
        #[callback] deposits: HashMap<AccountId, U128>,
    ) {
        let pool = Pool::from_config_with_assert(pool_id);
        let amounts = pool.extend_decimals(whole_amount.into());

        // All deposits must have enough of liquidity.
        for (token_id, amount) in amounts.clone() {
            let deposit = deposits.get(&token_id).unwrap_or(&U128::from(0u128)).0;
            require!(
                deposit >= amount,
                &format!("Not enough {} deposit: {} < {}", token_id, deposit, amount)
            );
        }

        let min_shares = NO_DEPOSIT.into();

        ext_ref_finance::add_stable_liquidity(
            pool.id,
            amounts.clone().map(|(_, amount)| amount.into()).collect(),
            min_shares,
            pool.ref_id.clone(),
            env::attached_deposit(),
            GAS_FOR_ADD_LIQUIDITY,
        )
        .as_return();
    }
}
