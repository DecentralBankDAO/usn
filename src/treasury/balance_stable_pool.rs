use std::collections::HashMap;

use crate::*;

use super::ft::REF_DEPOSIT_ACTION;
use super::gas::*;
use super::pool::{extend_decimals, remove_decimals, Pool};
use super::ref_finance::*;

use near_sdk::require;

#[near_bindgen]
impl Contract {
    #[payable]
    pub fn balance_stable_pool(&mut self, pool_id: u64) -> Promise {
        self.assert_owner_or_guardian();

        // 1st yoctoNEAR is for add_stable_liquidity/remove_stable_liquidity.
        require!(
            env::attached_deposit() > 0,
            "Requires attached deposit at least 1 yoctoNEAR"
        );

        let pool = Pool::from_config_with_assert(pool_id);

        ext_ref_finance::get_stable_pool(pool.id, pool.ref_id.clone(), 0, GAS_FOR_GET_DEPOSITS)
            .and(ext_ref_finance::get_deposits(
                env::current_account_id(),
                pool.ref_id,
                NO_DEPOSIT,
                GAS_FOR_GET_DEPOSITS,
            ))
            .then(ext_self::handle_start_balancing(
                pool.id,
                env::current_account_id(),
                env::attached_deposit(),
                GAS_FOR_FT_TRANSFER_CALL + GAS_FOR_WITHDRAW + GAS_FOR_FINISH_BURNING + GAS_SURPLUS,
            ))
    }
}

#[ext_contract(ext_self)]
trait RefFinanceHandler {
    #[private]
    #[payable]
    fn handle_start_balancing(
        &mut self,
        pool_id: u64,
        #[callback] info: StablePoolInfo,
        #[callback] deposits: HashMap<AccountId, U128>,
    ) -> PromiseOrValue<()>;

    #[private]
    fn finish_balancing_with_burn(&mut self, amount: U128);
}

trait RefFinanceHandler {
    fn handle_start_balancing(
        &mut self,
        pool_id: u64,
        info: StablePoolInfo,
        deposits: HashMap<AccountId, U128>,
    ) -> PromiseOrValue<()>;

    fn finish_balancing_with_burn(&mut self, amount: U128);
}

#[near_bindgen]
impl RefFinanceHandler for Contract {
    #[private]
    #[payable]
    fn handle_start_balancing(
        &mut self,
        pool_id: u64,
        #[callback] info: StablePoolInfo,
        #[callback] deposits: HashMap<AccountId, U128>,
    ) -> PromiseOrValue<()> {
        let pool = Pool::from_config_with_assert(pool_id);
        let count = info.amounts.len() as u128;

        require!(
            pool.tokens == info.token_account_ids,
            "Wrong pool structure"
        );

        let normalized_amounts =
            pool.decimals
                .iter()
                .zip(info.amounts)
                .map(move |(&decimals, amount)| {
                    if decimals < USN_DECIMALS {
                        extend_decimals(amount.into(), USN_DECIMALS - decimals)
                    } else if decimals > USN_DECIMALS {
                        remove_decimals(amount.into(), decimals)
                    } else {
                        amount.into()
                    }
                });

        let pairs = pool.tokens.iter().zip(normalized_amounts);

        let usn_id = env::current_account_id();

        let usn = pairs
            .clone()
            .find_map(|(token_id, amount)| {
                if token_id == &usn_id {
                    Some(amount)
                } else {
                    None
                }
            })
            .unwrap();
        let rest = pairs.clone().filter_map(|(token_id, amount)| {
            if token_id != &usn_id {
                Some(amount)
            } else {
                None
            }
        });

        let rest_count = rest.clone().count();

        require!(
            rest_count == pool.tokens.len() - 1,
            "Wrong number of tokens in the pool"
        );

        let average = rest.sum::<u128>() / (count - 1);

        if usn < average {
            // mint -> deposit -> add liquidity

            let to_add = average - usn;

            let usn_balance = self.token.internal_unwrap_balance_of(&usn_id);
            let usn_deposit = deposits.get(&usn_id).unwrap_or(&U128::from(0u128)).0;

            // Deposit.
            let maybe_deposit = if usn_deposit < to_add {
                let yet_to_deposit = to_add - usn_deposit;

                // Mint necessary USN amount.
                if usn_balance < yet_to_deposit {
                    let yet_to_mint = yet_to_deposit - usn_balance;
                    self.token.internal_deposit(&usn_id, yet_to_mint);
                    event::emit::ft_mint(&usn_id, yet_to_mint, None);
                }

                Some(self.token.internal_transfer_call(
                    &usn_id,
                    &pool.ref_id,
                    yet_to_deposit,
                    GAS_FOR_FT_TRANSFER_CALL,
                    None,
                    REF_DEPOSIT_ACTION.to_string(),
                ))
            } else {
                None
            };

            // Preserve the sequence of token amounts.
            let liquidity_amounts = info
                .token_account_ids
                .iter()
                .map(|token| {
                    if *token == usn_id {
                        // Add minted USN.
                        U128::from(to_add)
                    } else {
                        U128::from(0)
                    }
                })
                .collect::<Vec<U128>>();

            let min_shares = NO_DEPOSIT.into();

            // Add liquidity.
            let add_liquidity = ext_ref_finance::add_stable_liquidity(
                pool.id,
                liquidity_amounts,
                min_shares,
                pool.ref_id,
                env::attached_deposit(),
                GAS_FOR_ADD_LIQUIDITY,
            );

            // Chain the calls.
            if let Some(add_deposit) = maybe_deposit {
                add_deposit.then(add_liquidity).into()
            } else {
                add_liquidity.into()
            }
        } else if usn > average {
            // remove_liquidity -> withdraw -> burn.

            // Mint.
            let to_burn = usn - average;

            let burn_amounts = info
                .token_account_ids
                .iter()
                .map(|token| {
                    if *token == usn_id {
                        // Remove USN.
                        U128::from(to_burn)
                    } else {
                        U128::from(0)
                    }
                })
                .collect::<Vec<U128>>();

            let max_burn_shares = info.shares_total_supply;

            // Remove liquidity.
            ext_ref_finance::remove_liquidity_by_tokens(
                pool.id,
                burn_amounts,
                max_burn_shares,
                pool.ref_id.clone(),
                1,
                GAS_FOR_REMOVE_LIQUIDITY,
            )
            // Withdraw.
            .then(ext_ref_finance::withdraw(
                usn_id.clone(),
                to_burn.into(),
                None,
                pool.ref_id,
                1,
                GAS_FOR_WITHDRAW,
            ))
            // Burn.
            .then(ext_self::finish_balancing_with_burn(
                to_burn.into(),
                usn_id,
                NO_DEPOSIT,
                GAS_FOR_FINISH_BURNING,
            ))
            .into()
        } else {
            // Do nothing.
            PromiseOrValue::Value(())
        }
    }

    #[private]
    fn finish_balancing_with_burn(&mut self, amount: U128) {
        if is_promise_success() {
            self.token
                .internal_withdraw(&env::current_account_id(), amount.into());
            event::emit::ft_burn(&env::current_account_id(), amount.into(), None);
        }
    }
}
