use super::*;
use crate::*;

#[derive(Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, Serialize))]
#[serde(crate = "near_sdk::serde")]
pub struct AssetAmount {
    pub token_id: TokenId,
    /// The amount of tokens intended to be used for the action.
    /// If `None`, then the maximum amount will be tried.
    pub amount: Option<U128>,
    /// The maximum amount of tokens that can be used for the action.
    /// If `None`, then the maximum `available` amount will be used.
    pub max_amount: Option<U128>,
}

impl AssetAmount {
    pub fn new(token_id: &TokenId, amount: Balance) -> Self {
        Self {
            token_id: token_id.clone(),
            amount: Some(amount.into()),
            max_amount: None,
        }
    }
}

#[derive(Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, Serialize))]
#[serde(crate = "near_sdk::serde")]
pub enum Action {
    Withdraw(AssetAmount),
    IncreaseCollateral(AssetAmount),
    DecreaseCollateral(AssetAmount),
    Borrow(AssetAmount),
    BorrowUsn(U128),
    Repay(AssetAmount),
    RepayUsn(U128),
    Liquidate {
        account_id: AccountId,
        in_assets: Vec<AssetAmount>,
        out_assets: Vec<AssetAmount>,
    },
    /// If the sum of burrowed assets exceeds the collateral, the account will be liquidated
    /// using reserves.
    ForceClose {
        account_id: AccountId,
    },
}

impl Burrow {
    pub fn internal_execute(
        &mut self,
        account_id: &AccountId,
        account: &mut Account,
        actions: Vec<Action>,
        prices: Prices,
        token: &mut FungibleTokenFreeStorage,
    ) {
        self.internal_set_prices(&prices);
        let mut need_risk_check = false;
        let mut need_number_check = false;
        for action in actions {
            match action {
                Action::Withdraw(asset_amount) => {
                    account.add_affected_farm(FarmId::Supplied(asset_amount.token_id.clone()));
                    let amount = self.internal_withdraw(account, &asset_amount);
                    self.internal_ft_transfer(account_id, &asset_amount.token_id, amount);
                    events::emit::withdraw_started(&account_id, amount, &asset_amount.token_id);
                }
                Action::IncreaseCollateral(asset_amount) => {
                    need_number_check = true;
                    let amount = self.internal_increase_collateral(account, &asset_amount);
                    events::emit::increase_collateral(&account_id, amount, &asset_amount.token_id);
                }
                Action::DecreaseCollateral(asset_amount) => {
                    need_risk_check = true;
                    let mut account_asset =
                        account.internal_get_asset_or_default(&asset_amount.token_id);
                    let amount = self.internal_decrease_collateral(
                        &mut account_asset,
                        account,
                        &asset_amount,
                    );
                    account.internal_set_asset(&asset_amount.token_id, account_asset);
                    events::emit::decrease_collateral(&account_id, amount, &asset_amount.token_id);
                }
                Action::Borrow(asset_amount) => {
                    need_number_check = true;
                    need_risk_check = true;
                    account.add_affected_farm(FarmId::Supplied(asset_amount.token_id.clone()));
                    account.add_affected_farm(FarmId::Borrowed(asset_amount.token_id.clone()));
                    let amount = self.internal_borrow(account, &asset_amount);
                    events::emit::borrow(&account_id, amount, &asset_amount.token_id);
                }
                Action::BorrowUsn(amount) => {
                    need_number_check = true;
                    need_risk_check = true;

                    let amount = amount.into();
                    self.increase_usn_asset_supply(amount);

                    let asset_amount = AssetAmount::new(&usn_id(), amount);
                    self.internal_borrow(account, &asset_amount);
                    self.internal_withdraw(account, &asset_amount);

                    token.internal_deposit(&account.account_id, amount);
                    event::emit::ft_mint(&account.account_id, amount, Some("Borrow mint"));
                    events::emit::borrow(&account_id, amount, &usn_id());
                }
                Action::Repay(asset_amount) => {
                    let mut account_asset = account.internal_unwrap_asset(&asset_amount.token_id);
                    account.add_affected_farm(FarmId::Supplied(asset_amount.token_id.clone()));
                    account.add_affected_farm(FarmId::Borrowed(asset_amount.token_id.clone()));
                    let amount = self.internal_repay(&mut account_asset, account, &asset_amount);
                    events::emit::repay(&account_id, amount, &asset_amount.token_id);
                    account.internal_set_asset(&asset_amount.token_id, account_asset);
                }
                Action::RepayUsn(amount) => {
                    let (borrow_amount, repay_amount) =
                        self.withdraw_usn_interest(account, amount.0);

                    token.internal_withdraw(&account_id, repay_amount);
                    event::emit::ft_burn(&account_id, repay_amount, Some("Repay burn"));

                    self.internal_deposit(account, &usn_id(), borrow_amount);

                    // Get updated supplied amount after internal deposit
                    let mut account_asset = account.internal_unwrap_asset(&usn_id());
                    let asset_amount = AssetAmount::new(&usn_id(), borrow_amount);

                    self.internal_repay(&mut account_asset, account, &asset_amount);
                    self.decrease_usn_asset_supply(borrow_amount);

                    account.internal_set_asset(&usn_id(), account_asset);
                    events::emit::repay(&account_id, repay_amount, &usn_id());
                }
                Action::Liquidate {
                    account_id: liquidation_account_id,
                    in_assets,
                    out_assets,
                } => {
                    assert_ne!(
                        account_id, &liquidation_account_id,
                        "Can't liquidate yourself"
                    );
                    assert!(!in_assets.is_empty() && !out_assets.is_empty());
                    self.internal_liquidate(
                        account_id,
                        account,
                        &prices,
                        &liquidation_account_id,
                        in_assets,
                        out_assets,
                    );
                }
                Action::ForceClose {
                    account_id: liquidation_account_id,
                } => {
                    assert_ne!(
                        account_id, &liquidation_account_id,
                        "Can't liquidate yourself"
                    );
                    self.internal_force_close(&prices, &liquidation_account_id);
                }
            }
        }
        if need_number_check {
            assert!(
                account.collateral.len() + account.borrowed.len()
                    <= self.internal_config().max_num_assets as _
            );
        }
        if need_risk_check {
            assert!(
                self.compute_max_discount(account, &prices) == BigDecimal::zero(),
                "Not enough collateral to cover borrowed assets"
            );
        }

        self.internal_account_apply_affected_farms(account);
    }

    pub fn internal_deposit(
        &mut self,
        account: &mut Account,
        token_id: &TokenId,
        amount: Balance,
    ) -> Shares {
        let mut asset = self.internal_unwrap_asset(token_id);
        let mut account_asset = account.internal_get_asset_or_default(token_id);

        let shares: Shares = asset.supplied.amount_to_shares(amount, false);

        account_asset.deposit_shares(shares);
        account.internal_set_asset(&token_id, account_asset);

        asset.supplied.deposit(shares, amount);
        self.internal_set_asset(token_id, asset);

        shares
    }

    pub fn internal_withdraw(
        &mut self,
        account: &mut Account,
        asset_amount: &AssetAmount,
    ) -> Balance {
        let mut asset = self.internal_unwrap_asset(&asset_amount.token_id);
        assert!(
            asset.config.can_withdraw,
            "Withdrawals for this asset are not enabled"
        );

        let mut account_asset = account.internal_unwrap_asset(&asset_amount.token_id);

        let (shares, amount) =
            asset_amount_to_shares(&asset.supplied, account_asset.shares, &asset_amount, false);

        let available_amount = asset.available_amount();

        assert!(
            amount <= available_amount,
            "Withdraw error: Exceeded available amount {} of {}",
            available_amount,
            &asset_amount.token_id
        );

        account_asset.withdraw_shares(shares);
        account.internal_set_asset(&asset_amount.token_id, account_asset);

        asset.supplied.withdraw(shares, amount);
        self.internal_set_asset(&asset_amount.token_id, asset);

        amount
    }

    pub fn internal_increase_collateral(
        &mut self,
        account: &mut Account,
        asset_amount: &AssetAmount,
    ) -> Balance {
        let asset = self.internal_unwrap_asset(&asset_amount.token_id);
        assert!(
            asset.config.can_use_as_collateral,
            "Thi asset can't be used as a collateral"
        );

        let mut account_asset = account.internal_unwrap_asset(&asset_amount.token_id);

        let (shares, amount) =
            asset_amount_to_shares(&asset.supplied, account_asset.shares, &asset_amount, false);

        account_asset.withdraw_shares(shares);
        account.internal_set_asset(&asset_amount.token_id, account_asset);

        account.increase_collateral(&asset_amount.token_id, shares);

        amount
    }

    pub fn internal_decrease_collateral(
        &mut self,
        account_asset: &mut AccountAsset,
        account: &mut Account,
        asset_amount: &AssetAmount,
    ) -> Balance {
        let asset = self.internal_unwrap_asset(&asset_amount.token_id);
        let collateral_shares = account.internal_unwrap_collateral(&asset_amount.token_id);

        let (shares, amount) =
            asset_amount_to_shares(&asset.supplied, collateral_shares, &asset_amount, false);

        account.decrease_collateral(&asset_amount.token_id, shares);

        account_asset.deposit_shares(shares);

        amount
    }

    fn increase_usn_asset_supply(&mut self, amount: Balance) {
        let mut usn_account = self.internal_unwrap_account(&usn_id());
        self.internal_deposit(&mut usn_account, &usn_id(), amount);
        self.internal_set_account(&usn_id(), usn_account);
    }

    fn decrease_usn_asset_supply(&mut self, amount: Balance) -> Balance {
        let mut usn_account = self.internal_unwrap_account(&usn_id());
        let usn_asset_amount = AssetAmount::new(&usn_id(), amount);

        // Withdraw USN from supply
        self.internal_withdraw(&mut usn_account, &usn_asset_amount);
        self.internal_set_account(&usn_id(), usn_account);

        amount
    }

    pub fn internal_borrow(
        &mut self,
        account: &mut Account,
        asset_amount: &AssetAmount,
    ) -> Balance {
        let mut asset = self.internal_unwrap_asset(&asset_amount.token_id);
        assert!(asset.config.can_borrow, "This asset can't be used borrowed");

        let mut account_asset = account.internal_get_asset_or_default(&asset_amount.token_id);

        let available_amount = asset.available_amount();
        let max_borrow_shares = asset.borrowed.amount_to_shares(available_amount, false);

        let (borrowed_shares, amount) =
            asset_amount_to_shares(&asset.borrowed, max_borrow_shares, &asset_amount, true);

        assert!(
            amount <= available_amount,
            "Borrow error: Exceeded available amount {} of {}",
            available_amount,
            &asset_amount.token_id
        );

        let supplied_shares: Shares = asset.supplied.amount_to_shares(amount, false);

        asset.borrowed.deposit(borrowed_shares, amount);
        asset.supplied.deposit(supplied_shares, amount);
        self.internal_set_asset(&asset_amount.token_id, asset);

        account.increase_borrowed(&asset_amount.token_id, borrowed_shares);

        account_asset.deposit_shares(supplied_shares);
        account.internal_set_asset(&asset_amount.token_id, account_asset);

        amount
    }

    pub fn internal_repay(
        &mut self,
        account_asset: &mut AccountAsset,
        account: &mut Account,
        asset_amount: &AssetAmount,
    ) -> Balance {
        let mut asset = self.internal_unwrap_asset(&asset_amount.token_id);
        let available_borrowed_shares = account.internal_unwrap_borrowed(&asset_amount.token_id);

        let (mut borrowed_shares, mut amount) = asset_amount_to_shares(
            &asset.borrowed,
            available_borrowed_shares,
            &asset_amount,
            true,
        );

        let mut supplied_shares = asset.supplied.amount_to_shares(amount, true);
        if supplied_shares.0 > account_asset.shares.0 {
            supplied_shares = account_asset.shares;
            amount = asset.supplied.shares_to_amount(supplied_shares, false);
            if let Some(min_amount) = &asset_amount.amount {
                assert!(amount >= min_amount.0, "Not enough supplied balance");
            }
            assert!(amount > 0, "Repayment amount can't be 0");

            borrowed_shares = asset.borrowed.amount_to_shares(amount, false);
            assert!(borrowed_shares.0 > 0, "Shares can't be 0");
            assert!(borrowed_shares.0 <= available_borrowed_shares.0);
        }

        asset.supplied.withdraw(supplied_shares, amount);
        asset.borrowed.withdraw(borrowed_shares, amount);
        self.internal_set_asset(&asset_amount.token_id, asset);

        account.decrease_borrowed(&asset_amount.token_id, borrowed_shares);

        account_asset.withdraw_shares(supplied_shares);

        amount
    }

    fn withdraw_usn_interest(
        &mut self,
        account: &Account,
        repay_amount: Balance,
    ) -> (Balance, Balance) {
        let mut asset = self.internal_unwrap_asset(&usn_id());

        // Get user borrowed shares.
        let available_borrowed_shares = account.internal_unwrap_borrowed(&usn_id());

        // Get borrow amount from repay amount.
        let borrow_amount = self.without_usn_interest(repay_amount);

        // In case repay amount is bigger than actual borrow, repay amount = actual borrow.
        let borrowed_shares: U128 = std::cmp::min(
            asset.borrowed.amount_to_shares(borrow_amount, true).0,
            available_borrowed_shares.0,
        )
        .into();

        let borrow_amount = asset.borrowed.shares_to_amount(borrowed_shares, true);
        // Recalculate borrow interest according to new borrow amount.
        let borrow_interest = self.calculate_usn_interest(borrow_amount);

        // Decrease borrowed interest on repaid one.
        asset.borrowed.usn_interest -= borrow_interest;
        self.internal_set_asset(&usn_id(), asset);

        (borrow_amount, borrow_amount + borrow_interest)
    }

    pub fn internal_liquidate(
        &mut self,
        account_id: &AccountId,
        account: &mut Account,
        prices: &Prices,
        liquidation_account_id: &AccountId,
        in_assets: Vec<AssetAmount>,
        out_assets: Vec<AssetAmount>,
    ) {
        let mut liquidation_account = self.internal_unwrap_account(liquidation_account_id);

        let max_discount = self.compute_max_discount(&liquidation_account, &prices);
        assert!(
            max_discount > BigDecimal::zero(),
            "The liquidation account is not at risk"
        );

        let mut borrowed_repaid_sum = BigDecimal::zero();
        let mut collateral_taken_sum = BigDecimal::zero();

        for asset_amount in in_assets {
            let (amount, account_asset) = if is_usn(&asset_amount.token_id) {
                assert!(
                    is_usn(&account.account_id),
                    "Liquidation of USN asset should be done only by USN account"
                );
                let amount = asset_amount
                    .amount
                    .expect("Amount should be specified")
                    .into();
                let (borrow_amount, repay_amount) =
                    self.withdraw_usn_interest(&liquidation_account, amount);

                // Increase deposit for USN account
                self.internal_deposit(account, &usn_id(), borrow_amount);

                let mut account_asset = account.internal_unwrap_asset(&usn_id());
                let asset_amount = AssetAmount::new(&usn_id(), borrow_amount);

                self.internal_repay(&mut account_asset, &mut liquidation_account, &asset_amount);
                self.decrease_usn_asset_supply(borrow_amount);

                (repay_amount, account_asset)
            } else {
                liquidation_account
                    .add_affected_farm(FarmId::Borrowed(asset_amount.token_id.clone()));

                let mut account_asset = account.internal_unwrap_asset(&asset_amount.token_id);
                let amount = self.internal_repay(
                    &mut account_asset,
                    &mut liquidation_account,
                    &asset_amount,
                );
                (amount, account_asset)
            };

            account.internal_set_asset(&asset_amount.token_id, account_asset);
            let asset = self.internal_unwrap_asset(&asset_amount.token_id);

            borrowed_repaid_sum = borrowed_repaid_sum
                + BigDecimal::from_balance_price(
                    amount,
                    prices.get_unwrap(&asset_amount.token_id),
                    asset.config.extra_decimals,
                );
        }

        for asset_amount in out_assets {
            let asset = self.internal_unwrap_asset(&asset_amount.token_id);
            liquidation_account.add_affected_farm(FarmId::Supplied(asset_amount.token_id.clone()));
            let mut account_asset = account.internal_get_asset_or_default(&asset_amount.token_id);
            let amount = self.internal_decrease_collateral(
                &mut account_asset,
                &mut liquidation_account,
                &asset_amount,
            );
            account.internal_set_asset(&asset_amount.token_id, account_asset);

            collateral_taken_sum = collateral_taken_sum
                + BigDecimal::from_balance_price(
                    amount,
                    prices.get_unwrap(&asset_amount.token_id),
                    asset.config.extra_decimals,
                );
        }

        let discounted_collateral_taken = collateral_taken_sum * (BigDecimal::one() - max_discount);
        assert!(
            discounted_collateral_taken <= borrowed_repaid_sum,
            "Not enough balances repaid: discounted collateral {} > borrowed repaid sum {}",
            discounted_collateral_taken,
            borrowed_repaid_sum
        );

        let new_max_discount = self.compute_max_discount(&liquidation_account, &prices);
        assert!(
            new_max_discount > BigDecimal::zero(),
            "The liquidation amount is too large. The liquidation account should stay in risk"
        );
        assert!(
            new_max_discount < max_discount,
            "The health factor of liquidation account can't decrease. New discount {} < old discount {}",
            new_max_discount, max_discount
        );

        self.internal_account_apply_affected_farms(&mut liquidation_account);
        self.internal_set_account(liquidation_account_id, liquidation_account);

        events::emit::liquidate(
            &account_id,
            &liquidation_account_id,
            &collateral_taken_sum,
            &borrowed_repaid_sum,
        );
    }

    pub fn internal_force_close(&mut self, prices: &Prices, liquidation_account_id: &AccountId) {
        let config = self.internal_config();
        assert!(
            config.force_closing_enabled,
            "The force closing is not enabled"
        );

        let mut liquidation_account = self.internal_unwrap_account(liquidation_account_id);

        let mut borrowed_sum = BigDecimal::zero();
        let mut collateral_sum = BigDecimal::zero();

        let mut affected_farms = vec![];

        for (token_id, shares) in liquidation_account.collateral.drain() {
            let mut asset = self.internal_unwrap_asset(&token_id);
            let amount = asset.supplied.shares_to_amount(shares, false);
            asset.reserved += amount;
            asset.supplied.withdraw(shares, amount);

            collateral_sum = collateral_sum
                + BigDecimal::from_balance_price(
                    amount,
                    prices.get_unwrap(&token_id),
                    asset.config.extra_decimals,
                );
            self.internal_set_asset(&token_id, asset);
            affected_farms.push(FarmId::Supplied(token_id));
        }

        for (token_id, shares) in liquidation_account.borrowed.drain() {
            let mut asset = self.internal_unwrap_asset(&token_id);
            let amount = asset.borrowed.shares_to_amount(shares, true);
            assert!(
                asset.reserved >= amount,
                "Not enough {} in reserve",
                token_id
            );
            asset.reserved -= amount;
            asset.borrowed.withdraw(shares, amount);

            borrowed_sum = borrowed_sum
                + BigDecimal::from_balance_price(
                    amount,
                    prices.get_unwrap(&token_id),
                    asset.config.extra_decimals,
                );
            self.internal_set_asset(&token_id, asset);
            affected_farms.push(FarmId::Borrowed(token_id));
        }

        assert!(
            borrowed_sum > collateral_sum,
            "Total borrowed sum {} is not greater than total collateral sum {}",
            borrowed_sum,
            collateral_sum
        );
        liquidation_account.affected_farms.extend(affected_farms);

        self.internal_account_apply_affected_farms(&mut liquidation_account);
        self.internal_set_account(liquidation_account_id, liquidation_account);

        events::emit::force_close(&liquidation_account_id, &collateral_sum, &borrowed_sum);
    }

    pub fn compute_max_discount(&self, account: &Account, prices: &Prices) -> BigDecimal {
        if account.borrowed.is_empty() {
            return BigDecimal::zero();
        }

        let collateral_sum =
            account
                .collateral
                .iter()
                .fold(BigDecimal::zero(), |sum, (token_id, shares)| {
                    let asset = self.internal_unwrap_asset(&token_id);
                    let balance = asset.supplied.shares_to_amount(*shares, false)
                        + asset.supplied.usn_interest;
                    sum + BigDecimal::from_balance_price(
                        balance,
                        prices.get_unwrap(&token_id),
                        asset.config.extra_decimals,
                    )
                    .mul_ratio(asset.config.volatility_ratio)
                });

        let borrowed_sum =
            account
                .borrowed
                .iter()
                .fold(BigDecimal::zero(), |sum, (token_id, shares)| {
                    let asset = self.internal_unwrap_asset(&token_id);
                    let balance = asset.borrowed.shares_to_amount(*shares, true)
                        + asset.borrowed.usn_interest;
                    sum + BigDecimal::from_balance_price(
                        balance,
                        prices.get_unwrap(&token_id),
                        asset.config.extra_decimals,
                    )
                    .div_ratio(asset.config.volatility_ratio)
                });

        if borrowed_sum <= collateral_sum {
            BigDecimal::zero()
        } else {
            (borrowed_sum - collateral_sum) / borrowed_sum / BigDecimal::from(2u32)
        }
    }

    pub fn calculate_usn_interest(&self, amount: Balance) -> Balance {
        let asset = self.internal_unwrap_asset(&usn_id());

        u128_ratio(
            amount,
            asset.borrowed.usn_interest,
            asset.borrowed.balance,
            true,
        )
    }

    pub fn without_usn_interest(&self, repay_amount: Balance) -> Balance {
        let asset = self.internal_unwrap_asset(&usn_id());

        // Get borrow amount from repay amount.
        // borrow_amount / total_borrow = interest / total_interest
        // borrow_amount + interest = repay_amount
        // borrow_amount / total_borrowed = (repay_amount - borrow_amount) / total_interest.
        // borrow_amount = repay_amount * total_borrowed / (total_borrowed + total_interest).
        // e.g. repay_amount = 51, total_borrowed = 150, total_interest = 3
        // borrow_amount = (51 * 150) / (3 + 150) = 50
        u128_ratio(
            repay_amount,
            asset.borrowed.balance,
            (U256::from(asset.borrowed.usn_interest) + U256::from(asset.borrowed.balance))
                .as_u128(),
            false,
        )
    }
}

fn asset_amount_to_shares(
    pool: &Pool,
    available_shares: Shares,
    asset_amount: &AssetAmount,
    round_up: bool,
) -> (Shares, Balance) {
    let (shares, amount) = if let Some(amount) = &asset_amount.amount {
        (pool.amount_to_shares(amount.0, !round_up), amount.0)
    } else if let Some(max_amount) = &asset_amount.max_amount {
        let shares = std::cmp::min(
            available_shares.0,
            pool.amount_to_shares(max_amount.0, !round_up).0,
        )
        .into();
        (
            shares,
            std::cmp::min(pool.shares_to_amount(shares, round_up), max_amount.0),
        )
    } else {
        (
            available_shares,
            pool.shares_to_amount(available_shares, round_up),
        )
    };
    assert!(shares.0 > 0, "Shares can't be 0");
    assert!(amount > 0, "Amount can't be 0");
    (shares, amount)
}

impl Burrow {
    pub fn execute(&mut self, actions: Vec<Action>, token: &mut FungibleTokenFreeStorage) {
        let account_id = env::predecessor_account_id();
        let mut account = self.internal_unwrap_account(&account_id);
        self.internal_execute(&account_id, &mut account, actions, Prices::new(), token);
        self.internal_set_account(&account_id, account);
    }
}
