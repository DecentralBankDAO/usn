use super::*;
use crate::*;

pub mod emit {
    use super::*;

    #[derive(Serialize)]
    #[serde(crate = "near_sdk::serde")]
    struct AccountAmountToken<'a> {
        pub account_id: &'a AccountId,
        #[serde(with = "u128_dec_format")]
        pub amount: Balance,
        pub token_id: &'a TokenId,
    }

    fn log_event<T: Serialize>(event: &str, data: T) {
        let event = json!({
            "standard": "usn.burrow",
            "version": "1.0.0",
            "event": event,
            "data": [data]
        });

        log!("EVENT_JSON:{}", event.to_string());
    }

    pub fn deposit_to_reserve(account_id: &AccountId, amount: Balance, token_id: &TokenId) {
        log_event(
            "deposit_to_reserve",
            AccountAmountToken {
                account_id: &account_id,
                amount,
                token_id: &token_id,
            },
        );
    }

    pub fn deposit(account_id: &AccountId, amount: Balance, token_id: &TokenId) {
        log_event(
            "deposit",
            AccountAmountToken {
                account_id: &account_id,
                amount,
                token_id: &token_id,
            },
        );
    }

    pub fn withdraw_started(account_id: &AccountId, amount: Balance, token_id: &TokenId) {
        log_event(
            "withdraw_started",
            AccountAmountToken {
                account_id: &account_id,
                amount,
                token_id: &token_id,
            },
        );
    }

    pub fn withdraw_failed(account_id: &AccountId, amount: Balance, token_id: &TokenId) {
        log_event(
            "withdraw_failed",
            AccountAmountToken {
                account_id: &account_id,
                amount,
                token_id: &token_id,
            },
        );
    }

    pub fn withdraw_succeeded(account_id: &AccountId, amount: Balance, token_id: &TokenId) {
        log_event(
            "withdraw_succeeded",
            AccountAmountToken {
                account_id: &account_id,
                amount,
                token_id: &token_id,
            },
        );
    }

    pub fn increase_collateral(account_id: &AccountId, amount: Balance, token_id: &TokenId) {
        log_event(
            "increase_collateral",
            AccountAmountToken {
                account_id: &account_id,
                amount,
                token_id: &token_id,
            },
        );
    }

    pub fn decrease_collateral(account_id: &AccountId, amount: Balance, token_id: &TokenId) {
        log_event(
            "decrease_collateral",
            AccountAmountToken {
                account_id: &account_id,
                amount,
                token_id: &token_id,
            },
        );
    }

    pub fn borrow(account_id: &AccountId, amount: Balance, token_id: &TokenId) {
        log_event(
            "borrow",
            AccountAmountToken {
                account_id: &account_id,
                amount,
                token_id: &token_id,
            },
        );
    }

    pub fn repay(account_id: &AccountId, amount: Balance, token_id: &TokenId) {
        log_event(
            "repay",
            AccountAmountToken {
                account_id: &account_id,
                amount,
                token_id: &token_id,
            },
        );
    }

    pub fn liquidate(
        account_id: &AccountId,
        liquidation_account_id: &AccountId,
        collateral_sum: &BigDecimal,
        repaid_sum: &BigDecimal,
    ) {
        log_event(
            "liquidate",
            json!({
                "account_id": account_id,
                "liquidation_account_id": liquidation_account_id,
                "collateral_sum": collateral_sum,
                "repaid_sum": repaid_sum,
            }),
        );
    }

    pub fn force_close(
        liquidation_account_id: &AccountId,
        collateral_sum: &BigDecimal,
        repaid_sum: &BigDecimal,
    ) {
        log_event(
            "force_close",
            json!({
                "liquidation_account_id": liquidation_account_id,
                "collateral_sum": collateral_sum,
                "repaid_sum": repaid_sum,
            }),
        );
    }

    pub fn booster_stake(
        account_id: &AccountId,
        amount: Balance,
        duration: DurationSec,
        extra_x_booster_amount: Balance,
        booster_staking: &BoosterStaking,
    ) {
        log_event(
            "booster_stake",
            json!({
                "account_id": account_id,
                "booster_amount": U128(amount),
                "duration": duration,
                "x_booster_amount": U128(extra_x_booster_amount),
                "total_booster_amount": U128(booster_staking.staked_booster_amount),
                "total_x_booster_amount": U128(booster_staking.x_booster_amount),
            }),
        );
    }

    pub fn booster_unstake(account_id: &AccountId, booster_staking: &BoosterStaking) {
        log_event(
            "booster_unstake",
            json!({
                "account_id": account_id,
                "total_booster_amount": U128(booster_staking.staked_booster_amount),
                "total_x_booster_amount": U128(booster_staking.x_booster_amount),
            }),
        );
    }
}
