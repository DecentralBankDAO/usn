use super::*;
use crate::*;

#[derive(BorshSerialize, BorshDeserialize, Serialize, Default)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, Deserialize))]
#[serde(crate = "near_sdk::serde")]
pub struct BoosterStaking {
    /// The amount of Booster token staked.
    #[serde(with = "u128_dec_format")]
    pub staked_booster_amount: Balance,
    /// The amount of xBooster token.
    #[serde(with = "u128_dec_format")]
    pub x_booster_amount: Balance,
    /// When the staked Booster token can be unstaked in nanoseconds.
    #[serde(with = "u64_dec_format")]
    pub unlock_timestamp: u64,
}

impl Burrow {
    pub fn account_stake_booster(&mut self, amount: Option<U128>, duration: DurationSec) {
        let config = self.internal_config();

        assert!(
            duration >= config.minimum_staking_duration_sec
                && duration <= config.maximum_staking_duration_sec,
            "Duration is out of range"
        );

        let account_id = env::predecessor_account_id();
        let mut account = self.internal_unwrap_account(&account_id);

        let booster_token_id = config.booster_token_id.clone();

        // Computing and withdrawing amount from supplied.
        let mut asset = self.internal_unwrap_asset(&booster_token_id);
        let mut account_asset = account.internal_unwrap_asset(&booster_token_id);

        let (shares, amount) = if let Some(amount) = amount.map(|a| a.0) {
            (asset.supplied.amount_to_shares(amount, true), amount)
        } else {
            (
                account_asset.shares,
                asset.supplied.shares_to_amount(account_asset.shares, false),
            )
        };
        assert!(
            shares.0 > 0 && amount > 0,
            "The amount should be greater than zero"
        );

        account_asset.withdraw_shares(shares);
        account.internal_set_asset(&booster_token_id, account_asset);

        asset.supplied.withdraw(shares, amount);
        self.internal_set_asset(&booster_token_id, asset);

        // Computing amount of the new xBooster token and new unlock timestamp.
        let timestamp = env::block_timestamp();
        let new_duration_ns = sec_to_nano(duration);
        let new_unlock_timestamp_ns = timestamp + new_duration_ns;

        let mut booster_staking = account
            .booster_staking
            .take()
            .map(|mut booster_staking| {
                assert!(
                    booster_staking.unlock_timestamp <= new_unlock_timestamp_ns,
                    "The new staking duration is shorter than the current remaining staking duration"
                );

                let restaked_x_booster_amount = compute_x_booster_amount(
                    &config,
                    booster_staking.staked_booster_amount,
                    new_duration_ns,
                );

                booster_staking.x_booster_amount =
                    std::cmp::max(booster_staking.x_booster_amount, restaked_x_booster_amount);
                booster_staking
            })
            .unwrap_or_default();

        booster_staking.unlock_timestamp = new_unlock_timestamp_ns;
        booster_staking.staked_booster_amount += amount;
        let extra_x_booster_amount = compute_x_booster_amount(&config, amount, new_duration_ns);
        booster_staking.x_booster_amount += extra_x_booster_amount;

        events::emit::booster_stake(
            &account_id,
            amount,
            duration,
            extra_x_booster_amount,
            &booster_staking,
        );

        account.booster_staking.replace(booster_staking);

        account
            .affected_farms
            .extend(account.get_all_potential_farms());
        account.add_affected_farm(FarmId::Supplied(config.booster_token_id.clone()));
        self.internal_account_apply_affected_farms(&mut account);
        self.internal_set_account(&account_id, account);
    }

    pub fn account_unstake_booster(&mut self) {
        let config = self.internal_config();
        let account_id = env::predecessor_account_id();
        let mut account = self.internal_unwrap_account(&account_id);

        let timestamp = env::block_timestamp();
        let booster_staking = account
            .booster_staking
            .take()
            .expect("No staked booster token");
        assert!(
            booster_staking.unlock_timestamp <= timestamp,
            "The staking is not unlocked yet"
        );

        self.internal_deposit(
            &mut account,
            &config.booster_token_id,
            booster_staking.staked_booster_amount,
        );

        events::emit::booster_unstake(&account_id, &booster_staking);

        account
            .affected_farms
            .extend(account.get_all_potential_farms());
        self.internal_account_apply_affected_farms(&mut account);
        self.internal_set_account(&account_id, account);
    }
}

fn compute_x_booster_amount(config: &Config, amount: u128, duration_ns: Duration) -> u128 {
    amount
        + u128_ratio(
            amount,
            u128::from(
                config.x_booster_multiplier_at_maximum_staking_duration - MIN_BOOSTER_MULTIPLIER,
            ) * u128::from(duration_ns - to_nano(config.minimum_staking_duration_sec)),
            u128::from(to_nano(
                config.maximum_staking_duration_sec - config.minimum_staking_duration_sec,
            )) * u128::from(MIN_BOOSTER_MULTIPLIER),
            false,
        )
}
