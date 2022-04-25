use crate::*;
use near_contract_standards::fungible_token::core_impl::ext_fungible_token_receiver;
use near_contract_standards::fungible_token::events::FtTransfer;
use near_sdk::{log, require, IntoStorageKey, PromiseResult, StorageUsage};

const GAS_FOR_RESOLVE_TRANSFER: Gas = Gas(5_000_000_000_000);
const GAS_FOR_FT_TRANSFER_CALL: Gas = Gas(25_000_000_000_000 + GAS_FOR_RESOLVE_TRANSFER.0);

const NO_DEPOSIT: Balance = 0;

#[ext_contract(ext_ft_self)]
trait FungibleTokenResolver {
    fn ft_resolve_transfer(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> U128;
}

/// Implementation of a FungibleToken standard.
/// Allows to include NEP-141 compatible token to any contract.
/// There are next traits that any contract may implement:
///     - FungibleTokenCore -- interface with ft_transfer methods. FungibleToken provides methods for it.
///     - FungibleTokenMetaData -- return metadata for the token in NEP-148, up to contract to implement.
///
/// For example usage, see examples/fungible-token/src/lib.rs.
#[derive(BorshDeserialize, BorshSerialize)]
pub struct FungibleTokenFreeStorage {
    /// AccountID -> Account balance.
    pub accounts: LookupMap<AccountId, Balance>,

    /// Total supply of the all token.
    pub total_supply: Balance,

    /// DEPRECATED. Kept to match state without upgradability.
    /// Previously the maximum storage size of one account.
    pub _unused: StorageUsage,
}

impl FungibleTokenFreeStorage {
    pub fn new<S>(prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        Self {
            accounts: LookupMap::new(prefix),
            total_supply: 0,
            _unused: 0,
        }
    }

    pub fn internal_unwrap_balance_of(&self, account_id: &AccountId) -> Balance {
        self.accounts.get(account_id).unwrap_or(0)
    }

    pub fn internal_save_balance(&mut self, account_id: &AccountId, balance: Balance) {
        if balance > 0 {
            self.accounts.insert(account_id, &balance);
        } else {
            self.accounts.remove(account_id);
        }
    }

    pub fn internal_deposit(&mut self, account_id: &AccountId, amount: Balance) {
        let balance = self.internal_unwrap_balance_of(account_id);
        if let Some(new_balance) = balance.checked_add(amount) {
            self.internal_save_balance(account_id, new_balance);
            self.total_supply = self
                .total_supply
                .checked_add(amount)
                .unwrap_or_else(|| env::panic_str("Total supply overflow"));
        } else {
            env::panic_str("Balance overflow");
        }
    }

    pub fn internal_withdraw(&mut self, account_id: &AccountId, amount: Balance) {
        let balance = self.internal_unwrap_balance_of(account_id);
        if let Some(new_balance) = balance.checked_sub(amount) {
            self.internal_save_balance(account_id, new_balance);
            self.total_supply = self
                .total_supply
                .checked_sub(amount)
                .unwrap_or_else(|| env::panic_str("Total supply overflow"));
        } else {
            env::panic_str("The account doesn't have enough balance");
        }
    }

    pub fn internal_transfer(
        &mut self,
        sender_id: &AccountId,
        receiver_id: &AccountId,
        amount: Balance,
        memo: Option<String>,
    ) {
        require!(
            sender_id != receiver_id,
            "Sender and receiver should be different"
        );
        require!(amount > 0, "The amount should be a positive number");
        self.internal_withdraw(sender_id, amount);
        self.internal_deposit(receiver_id, amount);
        FtTransfer {
            old_owner_id: sender_id,
            new_owner_id: receiver_id,
            amount: &U128(amount),
            memo: memo.as_deref(),
        }
        .emit();
    }

    pub fn internal_transfer_call(
        &mut self,
        sender_id: &AccountId,
        receiver_id: &AccountId,
        amount: Balance,
        gas: Gas,
        memo: Option<String>,
        msg: String,
    ) -> Promise {
        require!(gas > GAS_FOR_FT_TRANSFER_CALL, "More gas is required");
        self.internal_transfer(sender_id, receiver_id, amount, memo);
        // Initiating receiver's call and the callback
        ext_fungible_token_receiver::ft_on_transfer(
            sender_id.clone(),
            amount.into(),
            msg,
            receiver_id.clone(),
            NO_DEPOSIT,
            gas - GAS_FOR_FT_TRANSFER_CALL,
        )
        .then(ext_ft_self::ft_resolve_transfer(
            sender_id.clone(),
            receiver_id.clone(),
            amount.into(),
            env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_RESOLVE_TRANSFER,
        ))
    }
}

impl FungibleTokenCore for FungibleTokenFreeStorage {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        assert_one_yocto();
        let sender_id = env::predecessor_account_id();
        let amount: Balance = amount.into();
        self.internal_transfer(&sender_id, &receiver_id, amount, memo);
    }

    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        assert_one_yocto();
        let sender_id = env::predecessor_account_id();
        let gas = env::prepaid_gas();
        self.internal_transfer_call(&sender_id, &receiver_id, amount.into(), gas, memo, msg)
            .into()
    }

    fn ft_total_supply(&self) -> U128 {
        self.total_supply.into()
    }

    fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        self.accounts.get(&account_id).unwrap_or(0).into()
    }
}

impl FungibleTokenFreeStorage {
    /// Internal method that returns the amount of burned tokens in a corner case when the sender
    /// has deleted (unregistered) their account while the `ft_transfer_call` was still in flight.
    /// Returns (Used token amount, Burned token amount)
    pub fn internal_ft_resolve_transfer(
        &mut self,
        sender_id: &AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> (u128, u128) {
        let amount: Balance = amount.into();

        // Get the unused amount from the `ft_on_transfer` call result.
        let unused_amount = match env::promise_result(0) {
            PromiseResult::NotReady => env::abort(),
            PromiseResult::Successful(value) => {
                if let Ok(unused_amount) = near_sdk::serde_json::from_slice::<U128>(&value) {
                    std::cmp::min(amount, unused_amount.0)
                } else {
                    amount
                }
            }
            PromiseResult::Failed => amount,
        };

        if unused_amount > 0 {
            let receiver_balance = self.internal_unwrap_balance_of(&receiver_id);
            if receiver_balance > 0 {
                let refund_amount = std::cmp::min(receiver_balance, unused_amount);
                self.internal_save_balance(&receiver_id, receiver_balance - refund_amount);

                let sender_balance = self.internal_unwrap_balance_of(&sender_id);
                self.internal_save_balance(&sender_id, sender_balance + refund_amount);
                log!(
                    "Refund {} from {} to {}",
                    refund_amount,
                    receiver_id,
                    sender_id
                );
                return (amount - refund_amount, 0);
            }
        }
        (amount, 0)
    }
}

impl FungibleTokenResolver for FungibleTokenFreeStorage {
    fn ft_resolve_transfer(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> U128 {
        self.internal_ft_resolve_transfer(&sender_id, receiver_id, amount)
            .0
            .into()
    }
}
