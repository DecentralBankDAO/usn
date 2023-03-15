use super::*;
use crate::*;
use near_contract_standards::fungible_token::core_impl::ext_fungible_token;
use near_sdk::json_types::U128;
use near_sdk::{is_promise_success, serde_json, PromiseOrValue};

const GAS_FOR_FT_TRANSFER: Gas = Gas(Gas::ONE_TERA.0 * 10);
const GAS_FOR_AFTER_FT_TRANSFER: Gas = Gas(Gas::ONE_TERA.0 * 20);

#[derive(Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, Serialize))]
#[serde(crate = "near_sdk::serde")]
pub enum TokenReceiverMsg {
    Execute { actions: Vec<Action> },
    DepositToReserve,
    Supply,
}

impl Burrow {
    /// Receives the transfer from the fungible token and executes a list of actions given in the
    /// message on behalf of the sender. The actions that can be executed should be limited to a set
    /// that doesn't require pricing.
    /// - Requires to be called by the fungible token account.
    pub fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let token_id = env::predecessor_account_id();
        let mut asset = self.internal_unwrap_asset(&token_id);
        assert!(
            asset.config.can_deposit,
            "Deposits for this asset are not enabled"
        );

        let amount = amount.0 * 10u128.pow(asset.config.extra_decimals as u32);

        // TODO: We need to be careful that only whitelisted tokens can call this method with a
        //     given set of actions. Or verify which actions are possible to do.
        let token_receiver_msg: TokenReceiverMsg =
            serde_json::from_str(&msg).expect("Can't parse TokenReceiverMsg");

        let actions: Vec<Action> = match token_receiver_msg {
            TokenReceiverMsg::Execute { actions } => actions,
            TokenReceiverMsg::DepositToReserve => {
                asset.reserved += amount;
                self.internal_set_asset(&token_id, asset);
                events::emit::deposit_to_reserve(&sender_id, amount, &token_id);
                return PromiseOrValue::Value(U128(0));
            }
            TokenReceiverMsg::Supply => vec![],
        };

        let mut account = self.internal_unwrap_account(&sender_id);
        account.add_affected_farm(FarmId::Supplied(token_id.clone()));
        self.internal_deposit(&mut account, &token_id, amount);
        events::emit::deposit(&sender_id, amount, &token_id);
        self.internal_execute(&sender_id, &mut account, actions, Prices::new());
        self.internal_set_account(&sender_id, account);

        PromiseOrValue::Value(U128(0))
    }

    pub fn internal_ft_transfer(
        &mut self,
        account_id: &AccountId,
        token_id: &TokenId,
        amount: Balance,
    ) -> Promise {
        let asset = self.internal_unwrap_asset(token_id);
        let ft_amount = amount / 10u128.pow(asset.config.extra_decimals as u32);
        ext_fungible_token::ft_transfer(
            account_id.clone(),
            ft_amount.into(),
            None,
            token_id.clone(),
            ONE_YOCTO,
            GAS_FOR_FT_TRANSFER,
        )
        .then(ext_self_burrow::after_ft_transfer(
            account_id.clone(),
            token_id.clone(),
            amount.into(),
            env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_AFTER_FT_TRANSFER,
        ))
    }
}

impl Burrow {
    pub fn after_ft_transfer(
        &mut self,
        account_id: AccountId,
        token_id: TokenId,
        amount: U128,
    ) -> bool {
        let promise_success = is_promise_success();
        if !promise_success {
            let mut account = self.internal_unwrap_account(&account_id);
            account.add_affected_farm(FarmId::Supplied(token_id.clone()));
            self.internal_deposit(&mut account, &token_id, amount.0);
            events::emit::withdraw_failed(&account_id, amount.0, &token_id);
            self.internal_set_account(&account_id, account);
        } else {
            events::emit::withdraw_succeeded(&account_id, amount.0, &token_id);
        }
        promise_success
    }
}
