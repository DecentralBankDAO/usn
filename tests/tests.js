'use strict';

const assert = require('assert').strict;
const config = require('./sandbox-setup').config;
const BN = require('bn.js');

const ONE_YOCTO = '1';
const GAS_FOR_CALL = '200000000000000'; // 200 TGas
const ONE_NEAR = '1000000000000000000000000';
const TEN_NEARS = '10000000000000000000000000';

describe('Smoke Test', function () {
  it('should get a version', async () => {
    const version = await global.aliceContract.version();
    assert.match(version, /2\..\../);
  });
});

describe('Anyone', function () {
  it('should get a name', async () => {
    const name = await global.aliceContract.name();
    assert.equal(name, 'USN');
  });

  it('should get a symbol', async () => {
    const symbol = await global.aliceContract.symbol();
    assert.equal(symbol, 'USN');
  });

  it('should get decimals', async () => {
    const decimals = await global.aliceContract.decimals();
    assert.equal(decimals, 18);
  });

  it('should get contract status', async () => {
    const status = await global.aliceContract.contract_status();
    assert.equal(status, 'Working');
  });

  it('should get an owner', async () => {
    const owner = await global.aliceContract.owner();
    assert.equal(owner, config.usnId);
  });

  it('should get a fake storage balance', async () => {
    const storage_balance = await global.aliceContract.storage_balance_of({
      account_id: 'fake.near',
    });
    assert.deepEqual(storage_balance, {
      total: '1250000000000000000000',
      available: '0',
    });
  });

  it('should get a commission', async () => {
    const commission = await global.aliceContract.commission();
    assert.deepEqual(commission,
      {
        v1: {
          near: '0',
          usn: '0',
        },
        v2: {
          usn: '0',
        },
      }
    );
  });

  it('should get commission rate', async () => {
    const commission_rate = await global.aliceContract.commission_rate({
      asset_id: config.usdtId,
    });
    assert.deepEqual(commission_rate, {
      deposit: 100,
      withdraw: 100
    });
  });
});

describe('Owner', function () {
  this.timeout(5000);

  it('should be able to assign guardians', async () => {
    await assert.doesNotReject(async () => {
      await global.usnContract.extend_guardians({
        args: { guardians: [config.aliceId] },
      });
    });
  });

  it('should get guardians', async () => {
    const guardians = await global.aliceContract.guardians();
    assert.deepEqual(guardians, [config.aliceId]);
  });

  it('should be able to remove guardians', async () => {
    await assert.doesNotReject(async () => {
      await global.usnContract.remove_guardians({
        args: { guardians: [config.aliceId] },
      });
    });
  });
});

describe('Owner', function () {
  this.timeout(7000);

  before(async () => {
    await global.usnContract.propose_new_owner({
      args: { proposed_owner_id: config.aliceId },
    });
    assert.equal(await global.usnContract.owner(), config.usnId);

    await global.aliceContract.accept_ownership({
      args: {},
    });
    assert.equal(await global.usnContract.owner(), config.aliceId);
  });

  it('can change ownership', async () => {
    await assert.rejects(async () => {
      await global.usnContract.propose_new_owner({
        args: { owner_id: config.usnId },
      });
    });
  });

  after(async () => {
    await global.aliceContract.propose_new_owner({
      args: { proposed_owner_id: config.usnId },
    });
    assert.equal(await global.usnContract.owner(), config.aliceId);

    await global.usnContract.accept_ownership({
      args: {},
    });
    assert.equal(await global.usnContract.owner(), config.usnId);
  });
});

describe('Owner', function () {
  this.timeout(5000);

  it('should fail to set deposit commission rate being called not by owner', async () => {
    await assert.rejects(
      async () => {
        await global.aliceContract.set_commission_rate({
          args: {
            asset_id: config.usdtId,
            rate: {
              deposit: 1000,
            }
          }
        });
      },
      (err) => {
        assert.match(err.message, /This method can be called only by owner/);
        return true;
      }
    );
  });

  it('should fail to set withdraw commission rate being called not by owner', async () => {
    await assert.rejects(
      async () => {
        await global.aliceContract.set_commission_rate({
          args: {
            asset_id: config.usdtId,
            rate: {
              withdraw: 1000,
            }
          }
        });
      },
      (err) => {
        assert.match(err.message, /This method can be called only by owner/);
        return true;
      }
    );
  });

  it('should be able to change deposit commission rate', async () => {
    await global.usnContract.set_commission_rate({
      args: {
        asset_id: config.usdtId,
        rate: {
          deposit: 2000,
        }
      }
    });
    const commission_rate = await global.aliceContract.commission_rate({
      asset_id: config.usdtId,
    });
    assert.equal(commission_rate.deposit, 2000);
  });

  it('should be able to change withdraw commission rate', async () => {
    await global.usnContract.set_commission_rate({
      args: {
        asset_id: config.usdtId,
        rate: {
          withdraw: 3000,
        }
      }
    });
    const commission_rate = await global.aliceContract.commission_rate({
      asset_id: config.usdtId,
    });
    assert.equal(commission_rate.withdraw, 3000);
  });

  after(async () => {
    await global.usnContract.set_commission_rate({
      args: {
        asset_id: config.usdtId,
        rate: {
          deposit: 100,
          withdraw: 100,
        }
      }
    });
  });
});

describe('Guardian', function () {
  this.timeout(10000);

  before(async () => {
    await global.usnContract.extend_guardians({
      args: { guardians: [config.aliceId] },
    });
  });

  it('should be able to pause the contract', async () => {
    await assert.doesNotReject(async () => {
      await global.aliceContract.pause({ args: {}, amount: ONE_YOCTO });
      assert.equal(await global.aliceContract.contract_status(), 'Paused');
    });

    await assert.rejects(async () => {
      await global.aliceContract.ft_transfer({
        args: { receiver_id: 'any', amount: '1' },
      });
    });

    await assert.doesNotReject(async () => {
      await global.usnContract.resume({ args: {} });
      assert.equal(await global.aliceContract.contract_status(), 'Working');
    });
  });

  after(async () => {
    await global.usnContract.remove_guardians({
      args: { guardians: [config.aliceId] },
    });
  });
});

describe('Owner', function () {
  this.timeout(15000);

  before(async () => {
    await global.usnContract.propose_new_owner({
      args: { proposed_owner_id: config.aliceId },
    });
    assert.equal(await global.usnContract.owner(), config.usnId);

    await global.aliceContract.accept_ownership({
      args: {},
    });
    assert.equal(await global.usnContract.owner(), config.aliceId);
  });


  it('should fail to buy USN being called not by owner', async () => {
    await assert.rejects(
      async () => {
        await global.usnContract.mint_by_near({
          args: {
            collateral_ratio: 100,
          }
        });
      },
      (err) => {
        assert.match(err.message, /This method can be called only by owner/);
        return true;
      }
    );
  });

  it('should fail to buy USN being due to low collateral ratio', async () => {
    await assert.rejects(
      async () => {
        await global.aliceContract.mint_by_near({
          args: {
            collateral_ratio: 99,
          }
        });
      },
      (err) => {
        assert.match(err.message, /Collateral ratio is out of bounds/);
        return true;
      }
    );
  });

  it('should fail to buy USN being due to exceeded collateral ratio', async () => {
    await assert.rejects(
      async () => {
        await global.aliceContract.mint_by_near({
          args: {
            collateral_ratio: 1001,
          }
        });
      },
      (err) => {
        assert.match(err.message, /Collateral ratio is out of bounds/);
        return true;
      }
    );
  });

  it('should be able to mint USN for NEAR with 100% collateralization', async () => {
    const nearOwnerBalanceBefore = await global.aliceAccount.state();
    const nearUsnBalanceBefore = await global.usnAccount.state();
    const usnBalanceBefore = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId,
    });

    await global.aliceContract.mint_by_near({
      args: {
        collateral_ratio: 100,
      },
      amount: TEN_NEARS,
      gas: GAS_FOR_CALL,
    });

    const nearOwnerBalanceAfter = await global.aliceAccount.state();
    const nearUsnBalanceAfter = await global.usnAccount.state();
    const usnBalanceAfter = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId,
    });

    assert(new BN(nearUsnBalanceAfter.amount)
      .sub(new BN(nearUsnBalanceBefore.amount))
      .gt(new BN(TEN_NEARS))
    );
    assert(new BN(nearOwnerBalanceBefore.amount)
      .sub(new BN(nearOwnerBalanceAfter.amount))
      .gt(new BN(TEN_NEARS))
    );
    assert.equal(new BN(usnBalanceAfter)
      .sub(new BN(usnBalanceBefore)).toString(),
      '111439000000000000000' // 111.43$
    );
  });

  it('should be able to mint USN for NEAR with 210% collateralization', async () => {
    const nearOwnerBalanceBefore = await global.aliceAccount.state();
    const nearUsnBalanceBefore = await global.usnAccount.state();
    const usnBalanceBefore = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId,
    });

    await global.aliceContract.mint_by_near({
      args: {
        collateral_ratio: 210,
      },
      amount: TEN_NEARS,
      gas: GAS_FOR_CALL,
    });

    const nearOwnerBalanceAfter = await global.aliceAccount.state();
    const nearUsnBalanceAfter = await global.usnAccount.state();
    const usnBalanceAfter = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId,
    });

    assert(new BN(nearUsnBalanceAfter.amount)
      .sub(new BN(nearUsnBalanceBefore.amount))
      .gt(new BN(TEN_NEARS))
    );
    assert(new BN(nearOwnerBalanceBefore.amount)
      .sub(new BN(nearOwnerBalanceAfter.amount))
      .gt(new BN(TEN_NEARS))
    );
    assert.equal(new BN(usnBalanceAfter)
      .sub(new BN(usnBalanceBefore)).toString(),
      '53066190476190476190' // 53.06$
    );
  });

  it('should be able to mint USN for NEAR with 1000% collateralization', async () => {
    const nearOwnerBalanceBefore = await global.aliceAccount.state();
    const nearUsnBalanceBefore = await global.usnAccount.state();
    const usnBalanceBefore = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId,
    });

    await global.aliceContract.mint_by_near({
      args: {
        collateral_ratio: 1000,
      },
      amount: TEN_NEARS,
      gas: GAS_FOR_CALL,
    });

    const nearOwnerBalanceAfter = await global.aliceAccount.state();
    const nearUsnBalanceAfter = await global.usnAccount.state();
    const usnBalanceAfter = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId,
    });

    assert(new BN(nearUsnBalanceAfter.amount)
      .sub(new BN(nearUsnBalanceBefore.amount))
      .gt(new BN(TEN_NEARS))
    );
    assert(new BN(nearOwnerBalanceBefore.amount)
      .sub(new BN(nearOwnerBalanceAfter.amount))
      .gt(new BN(TEN_NEARS))
    );
    assert.equal(new BN(usnBalanceAfter)
      .sub(new BN(usnBalanceBefore)).toString(),
      '11143900000000000000' // 11.14$
    );
  });

  after(async () => {
    const aliceBalance = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId,
    });

    // Flush balances and force registration removal.

    if (aliceBalance != '0') {
      await global.aliceContract.ft_transfer({
        args: {
          receiver_id: 'any',
          amount: aliceBalance,
        },
        amount: ONE_YOCTO,
      });
    }

    await global.aliceContract.propose_new_owner({
      args: { proposed_owner_id: config.usnId },
    });
    assert.equal(await global.usnContract.owner(), config.aliceId);

    await global.usnContract.accept_ownership({
      args: {},
    });
    assert.equal(await global.usnContract.owner(), config.usnId);
  });
});

describe('User', async function () {
  this.timeout(15000);

  it('can NOT withdraw before depositing', async () => {
    await assert.rejects(async () => {
      await global.usnContract.withdraw({
        args: { amount: 1 },
        amount: ONE_YOCTO,
      });
    });
  });

  it('should exchange USN for USDT with correct price', async () => {
    const usdtBefore = await global.aliceUsdt.ft_balance_of({
      account_id: config.aliceId,
    });

    const usnBefore = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId
    });

    // Alice gets USN.
    await global.aliceUsdt.ft_transfer_call({
      args: {
        receiver_id: config.usnId,
        amount: '1000000000000',
        msg: '',
      },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    const usdtAfter = await global.aliceUsdt.ft_balance_of({
      account_id: config.aliceId,
    });
    const usnAfter = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId
    });
    const commission = await global.usnContract.commission();

    assert.equal(usdtBefore, usdtAfter);
    assert.equal(usnBefore, usnAfter);
    assert.equal(commission.v2.usn, '0');

    await assert.rejects(
      async () => {
        // Alice swaps USN to USDT.
        await global.aliceContract.withdraw({
          args: {
            amount: '999900000000000000000000',
          },
          amount: ONE_YOCTO,
          gas: GAS_FOR_CALL,
        });
      },
      (err) => {
        assert.match(err.message, /The account doesn't have enough balance/);
        return true;
      }
    );
  });

  it('should deposit USDT and withdraw USDC', async () => {
    // Fill Alice account with USDT
    await global.usdtContract.ft_transfer({
      args: {
        receiver_id: config.aliceId,
        amount: '1000000000000',
        msg: '',
      },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    const usdtBefore = await global.aliceUsdt.ft_balance_of({
      account_id: config.aliceId,
    });

    const usnBefore = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId
    });

    // Alice gets USN.
    await global.aliceUsdt.ft_transfer_call({
      args: {
        receiver_id: config.usnId,
        amount: '1000000000000',
        msg: '',
      },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    const usdtAfter = await global.aliceUsdt.ft_balance_of({
      account_id: config.aliceId,
    });
    const usnAfter = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId
    });
    const commission = await global.usnContract.commission();

    assert.equal(usdtBefore, usdtAfter);
    assert.equal(usnBefore, usnAfter);
    assert.equal(commission.v2.usn, '0');

    await global.usnContract.add_stable_asset({
      args: {
        asset_id: config.usdcId,
        decimals: 6,
      }
    });
    await global.usnContract.set_commission_rate({
      args: {
        asset_id: config.usdcId,
        rate: {
          withdraw: 2000,
        }
      }
    });

    await assert.rejects(
      async () => {
        // Alice swaps USN to USDC.
        await global.aliceContract.withdraw({
          args: {
            asset_id: config.usdcId,
            amount: '999900000000000000000000',
          },
          amount: ONE_YOCTO,
          gas: GAS_FOR_CALL,
        });
      },
      (err) => {
        assert.match(err.message, /The account doesn't have enough balance/);
        return true;
      }
    );
  });

  it('should have withdrawn all USN to get unregistered', async () => {
    await assert.rejects(
      async () => {
        await global.aliceContract.ft_transfer({
          args: { receiver_id: 'any', amount: '1' },
          amount: ONE_YOCTO,
        });
      },
      (err) => {
        assert.match(err.message, /The account doesn't have enough balance/);
        return true;
      }
    );
  });

  it('should not withdraw having no token account', async () => {
    // Alice gets USN.
    await global.aliceUsdt.ft_transfer_call({
      args: {
        receiver_id: config.usnId,
        amount: '2000000',
        msg: '',
      },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    await assert.rejects(
      async () => {
        // Deposit $USN on ref.finance.
        await global.aliceContract.ft_transfer({
          args: {
            receiver_id: config.carolId,
            amount: '1000000000000000000',
            msg: '',
          },
          amount: ONE_YOCTO,
          gas: GAS_FOR_CALL,
        });
      },
      (err) => {
        assert.match(err.message, /The account doesn't have enough balance/);
        return true;
      }
    );

    assert.equal(
      await global.carolContract.ft_balance_of({ account_id: config.carolId }),
      '0'
    );

    // Try to withdraw
    await assert.rejects(
      async () => {
        await global.carolContract.withdraw({
          args: {
            amount: '1000000000000000000',
          },
          amount: ONE_YOCTO,
          gas: GAS_FOR_CALL,
        });
      },
      (err) => {
        assert.match(err.message, /The account doesn't have enough balance/);
        return true;
      }
    );

    const commissionAfter = await global.usnContract.commission();

    assert.equal(
      await global.carolUsdt.ft_balance_of({ account_id: config.carolId }),
      '0'
    );
    assert.equal(
      await global.carolContract.ft_balance_of({ account_id: config.carolId }),
      '0'
    );
    assert.equal(commissionAfter.v2.usn, '0');
  });

  it('should fail to deposit due to low attached gas', async () => {
    const usdtBefore = await global.aliceUsdt.ft_balance_of({
      account_id: config.aliceId,
    });
    const usnBefore = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId
    });
    const commissionBefore = await global.usnContract.commission();

    // Alice gets USN.
    await assert.rejects(
      async () => {
        await global.aliceUsdt.ft_transfer_call({
          args: {
            receiver_id: config.usnId,
            amount: usdtBefore,
            msg: '',
          },
          amount: ONE_YOCTO,
          gas: "30000000000000", // 30 TGas 
        });
      },
      (err) => {
        assert.match(err.message, /FunctionCallZeroAttachedGas/);
        return true;
      }
    );

    const usdtAfter = await global.aliceUsdt.ft_balance_of({
      account_id: config.aliceId,
    });
    const usnAfter = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId
    });
    const commissionAfter = await global.usnContract.commission();

    assert.equal(usdtBefore, usdtAfter);
    assert.equal(usnBefore, usnAfter);
    assert.equal(commissionBefore.v2.usn, commissionAfter.v2.usn);
  });

  it('should not deposit due to not enough attached gas', async () => {
    const usdtBefore = await global.aliceUsdt.ft_balance_of({
      account_id: config.aliceId,
    });
    const usnBefore = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId
    });
    const commissionBefore = await global.usnContract.commission();

    // Alice gets USN.
    await global.aliceUsdt.ft_transfer_call({
      args: {
        receiver_id: config.usnId,
        amount: usdtBefore,
        msg: '',
      },
      amount: ONE_YOCTO,
      gas: "31000000000000", // 31 TGas 
    });

    const usdtAfter = await global.aliceUsdt.ft_balance_of({
      account_id: config.aliceId,
    });
    const usnAfter = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId
    });
    const commissionAfter = await global.usnContract.commission();

    assert.equal(usdtBefore, usdtAfter);
    assert.equal(usnBefore, usnAfter);
    assert.equal(commissionBefore.v2.usn, commissionAfter.v2.usn);
  });

  it('should fail to withdraw due to low attached gas', async () => {
    const usdtAmount = await global.aliceUsdt.ft_balance_of({
      account_id: config.aliceId,
    });
    // Alice gets USN.
    await global.aliceUsdt.ft_transfer_call({
      args: {
        receiver_id: config.usnId,
        amount: usdtAmount,
        msg: '',
      },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    const usdtBefore = await global.aliceUsdt.ft_balance_of({
      account_id: config.aliceId,
    });
    const usnBefore = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId
    });
    const commissionBefore = await global.usnContract.commission();

    // Alice swaps USN to USDT.
    await assert.rejects(
      async () => {
        await global.aliceContract.withdraw({
          args: {
            amount: usnBefore,
          },
          amount: ONE_YOCTO,
          gas: "30000000000000", // 30 TGas 
        });
      },
      (err) => {
        assert.match(err.message, /Not enough USN: specified amount exchanges to 0 tokens/);
        return true;
      }
    );

    const usdtAfter = await global.aliceUsdt.ft_balance_of({
      account_id: config.aliceId,
    });
    const usnAfter = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId
    });
    const commissionAfter = await global.usnContract.commission();

    assert.equal(usdtBefore, usdtAfter);
    assert.equal(usnBefore, usnAfter);
    assert.equal(commissionBefore.v2.usn, commissionAfter.v2.usn);
  });

  after(async () => {
    const aliceBalance = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId,
    });

    // Flush balances and force registration removal.

    if (aliceBalance != '0') {
      await global.aliceContract.ft_transfer({
        args: {
          receiver_id: 'any',
          amount: aliceBalance,
        },
        amount: ONE_YOCTO,
      });
    }
  });
});

describe('Commission transfer', function () {
  this.timeout(5000);

  it('should fail being called not by owner', async () => {
    await assert.rejects(
      async () => {
        await global.aliceContract.transfer_commission({
          args: {
            account_id: config.aliceId,
            amount: '10000000000'
          }
        });
      },
      (err) => {
        assert.match(err.message, /This method can be called only by owner/);
        return true;
      }
    );
  });

  it('should fail trying to transfer 0 commission amount', async () => {
    await assert.rejects(
      async () => {
        await global.usnContract.transfer_commission({
          args: {
            account_id: config.aliceId,
            amount: '0',
          }
        });
      },
      (err) => {
        assert.match(err.message, /Amount should be positive/);
        return true;
      }
    );
  });

  it('should transfer certain commission amount', async () => {
    const commissionBefore = await global.usnContract.commission();
    const transfer_amount = '10000000000';

    await assert.rejects(
      async () => {
        await global.usnContract.transfer_commission({
          args: {
            account_id: config.aliceId,
            amount: transfer_amount,
          }
        });
      },
      (err) => {
        assert.match(err.message, /Exceeded the commission v2 amount/);
        return true;
      }
    );

    const commissionAfter = await global.usnContract.commission();
    const userBalance = await global.usnContract.ft_balance_of({
      account_id: config.aliceId,
    });

    assert.equal(userBalance, '0');
    assert.equal(commissionBefore.v2.usn, '0');
    assert.equal(commissionAfter.v2.usn, '0');
  });

  it('should fail trying to transfer more than account has', async () => {
    await assert.rejects(
      async () => {
        await global.usnContract.transfer_commission({
          args: {
            account_id: config.aliceId,
            amount: '1000000000000000000000000',
          }
        });
      },
      (err) => {
        assert.match(err.message, /Exceeded the commission v2 amount/);
        return true;
      }
    );
  });

  it('should transfer all commission amount', async () => {
    const commissionBefore = await global.usnContract.commission();

    await assert.rejects(
      async () => {
        await global.usnContract.transfer_commission({
          args: {
            account_id: config.aliceId,
            amount: commissionBefore.v2.usn,
          }
        });
      },
      (err) => {
        assert.match(err.message, /Amount should be positive/);
        return true;
      }
    );

    const commissionAfter = await global.usnContract.commission();
    const userBalanceAfter = await global.usnContract.ft_balance_of({
      account_id: config.aliceId,
    });

    assert.equal(commissionBefore.v2.usn, '0');
    assert.equal(commissionAfter.v2.usn, '0');
    assert.equal(userBalanceAfter, '0');
  });

  it('should fail as there is no commission', async () => {
    await assert.rejects(
      async () => {
        await global.usnContract.transfer_commission({
          args: {
            account_id: config.aliceId,
            amount: '10000000000',
          }
        });
      },
      (err) => {
        assert.match(err.message, /Exceeded the commission v2 amount/);
        return true;
      }
    );
  });
});

describe('Withdraw Stable Pool', async function () {
  this.timeout(30000);

  const MAX_TRANSFER_COST = '780000000000000000001';

  before(async () => {
    // Fill up USN account with USDT token: $1000000.
    await global.usdtContract.ft_transfer({
      args: { receiver_id: config.usnId, amount: '1000000000000' },
      amount: ONE_YOCTO,
    });

    // Add stable liquidity to a stable pool.
    await global.usnContract.transfer_stable_liquidity({
      args: { pool_id: 0, whole_amount: '1000000' },
      amount: MAX_TRANSFER_COST,
      gas: GAS_FOR_CALL,
    });
  });

  it('should fail being called not by owner or guardian', async () => {
    await assert.rejects(
      async () => {
        await global.aliceContract.withdraw_stable_pool({
          args: {},
          gas: GAS_FOR_CALL,
        });
      },
      (err) => {
        assert.match(err.message, /This method can be called only by owner/);
        return true;
      }
    );
  });

  it('should fail trying to withdraw 100% because there is only 1 participant', async () => {
    await assert.rejects(
      async () => {
        await global.usnContract.withdraw_stable_pool({
          args: { percent: 100 },
          amount: 3 * ONE_YOCTO,
          gas: GAS_FOR_CALL,
        });
      },
      (err) => {
        assert.match(err.message, /Callback computation 0 was not successful/);
        return true;
      }
    );
  });

  it('should fail trying to withdraw 101% of liquidity', async () => {
    await assert.rejects(
      async () => {
        await global.usnContract.withdraw_stable_pool({
          args: { percent: 101 },
          amount: 3 * ONE_YOCTO,
          gas: GAS_FOR_CALL,
        });
      },
      (err) => {
        assert.match(err.message, /Maximum 100%/);
        return true;
      }
    );
  });

  it('should withdraw 99% of shares', async () => {
    const poolInfoBefore = await global.refContract.get_stable_pool({
      pool_id: 0,
    });

    await global.usnContract.withdraw_stable_pool({
      args: { percent: 99 },
      amount: 3 * ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    const poolInfoAfter = await global.refContract.get_stable_pool({
      pool_id: 0,
    });

    const usnDeposit = await global.refContract.get_deposit({
      account_id: config.usnId,
      token_id: config.usnId,
    });
    const usdtDeposit = await global.refContract.get_deposit({
      account_id: config.usnId,
      token_id: config.usdtId,
    });
    assert.equal(usnDeposit, '0');
    assert.equal(usdtDeposit, '0');

    assert(
      new BN(poolInfoBefore.amounts[0]).gt(new BN(poolInfoAfter.amounts[0]))
    );

    assert(
      new BN(poolInfoBefore.amounts[1]).gt(new BN(poolInfoAfter.amounts[1]))
    );

    const poolUsn99Percent = new BN(poolInfoBefore.amounts[0])
      .mul(new BN(991))
      .div(new BN(1000));
    const poolUsn98Percent = new BN(poolInfoBefore.amounts[0])
      .mul(new BN(98))
      .div(new BN(100));
    const usnAmountDiff = new BN(poolInfoBefore.amounts[0]).sub(
      new BN(poolInfoAfter.amounts[0])
    );

    // Withdrawn 98% < USN < 99%
    assert(usnAmountDiff.gt(new BN(poolUsn98Percent)));
    assert(usnAmountDiff.lt(new BN(poolUsn99Percent)));

    const poolUsdt99Percent = new BN(poolInfoBefore.amounts[1])
      .mul(new BN(991))
      .div(new BN(1000));
    const poolUsdt98Percent = new BN(poolInfoBefore.amounts[1])
      .mul(new BN(98))
      .div(new BN(100));
    const usdtAmountDiff = new BN(poolInfoBefore.amounts[1]).sub(
      new BN(poolInfoAfter.amounts[1])
    );

    // Withdrawn 98% < USDT < 99%
    assert(usdtAmountDiff.gt(new BN(poolUsdt98Percent)));
    assert(usdtAmountDiff.lt(new BN(poolUsdt99Percent)));
  });

  it('should withdraw 5% of shares', async () => {
    const poolInfoBefore = await global.refContract.get_stable_pool({
      pool_id: 0,
    });

    await global.usnContract.withdraw_stable_pool({
      args: {},
      amount: 3 * ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    const poolInfoAfter = await global.refContract.get_stable_pool({
      pool_id: 0,
    });

    const usnDeposit = await global.refContract.get_deposit({
      account_id: config.usnId,
      token_id: config.usnId,
    });
    const usdtDeposit = await global.refContract.get_deposit({
      account_id: config.usnId,
      token_id: config.usdtId,
    });
    assert.equal(usnDeposit, '0');
    assert.equal(usdtDeposit, '0');

    assert(
      new BN(poolInfoBefore.amounts[0]).gt(new BN(poolInfoAfter.amounts[0]))
    );

    assert(
      new BN(poolInfoBefore.amounts[1]).gt(new BN(poolInfoAfter.amounts[1]))
    );

    // USN: after < before.
    const poolUsn5Percent = new BN(poolInfoBefore.amounts[0])
      .mul(new BN(5))
      .div(new BN(100));

    const poolUsn49Percent = new BN(poolInfoBefore.amounts[0])
      .mul(new BN(49))
      .div(new BN(1000));
    const usnAmountDiff = new BN(poolInfoBefore.amounts[0]).sub(
      new BN(poolInfoAfter.amounts[0])
    );

    assert(usnAmountDiff.gt(new BN(poolUsn49Percent)));
    assert(usnAmountDiff.lte(new BN(poolUsn5Percent)));

    // USDT: after < before.
    const poolUsdt5Percent = new BN(poolInfoBefore.amounts[1])
      .mul(new BN(5))
      .div(new BN(100));

    const poolUsdt49Percent = new BN(poolInfoBefore.amounts[1])
      .mul(new BN(49))
      .div(new BN(1000));
    const usdtAmountDiff = new BN(poolInfoBefore.amounts[1]).sub(
      new BN(poolInfoAfter.amounts[1])
    );

    assert(usdtAmountDiff.gt(new BN(poolUsdt49Percent)));
    assert(usdtAmountDiff.lte(new BN(poolUsdt5Percent)));
  });

  it('should fail being called with lower gas and pools should not be changed', async () => {
    const GAS_FOR_REMOVE_LIQUIDITY_AND_1_WTHDRAW = '72000000000000';
    const poolInfoBefore = await global.refContract.get_stable_pool({
      pool_id: 0,
    });
    await assert.rejects(
      async () => {
        await global.usnContract.withdraw_stable_pool({
          args: {},
          amount: 3 * ONE_YOCTO,
          gas: GAS_FOR_REMOVE_LIQUIDITY_AND_1_WTHDRAW,
        });
      },
      (err) => {
        assert.match(err.message, /Exceeded the prepaid gas./);
        return true;
      }
    );
    const poolInfoAfter = await global.refContract.get_stable_pool({
      pool_id: 0,
    });

    assert(
      new BN(poolInfoBefore.amounts[0]).eq(new BN(poolInfoAfter.amounts[0]))
    );
    assert(
      new BN(poolInfoBefore.amounts[1]).eq(new BN(poolInfoAfter.amounts[1]))
    );
  });
});

describe('Staking Pool', async function () {
  this.timeout(30000);

  it('should fail being called not by owner', async () => {
    await assert.rejects(
      async () => {
        await global.aliceContract.stake({
          args: {
            amount: ONE_NEAR,
            pool_id: config.poolId,
          },
          gas: GAS_FOR_CALL,
        });
      },
      (err) => {
        assert.match(err.message, /This method can be called only by owner/);
        return true;
      }
    );
  });

  it('should fail as nothing was staked', async () => {
    await assert.rejects(
      async () => {
        await global.usnContract.unstake({
          args: {
            amount: ONE_NEAR,
            pool_id: config.poolId,
          },
          gas: GAS_FOR_CALL,
        });
      },
      (err) => {
        assert.match(err.message, /Unstaking amount should be positive/);
        return true;
      }
    );
  });

  it('should stake certain amount of NEAR', async () => {
    const nearBalanceBefore = await global.usnAccount.state();
    const stakeAmount = "10000000000000000000000000"; // 10 NEAR

    await global.usnContract.stake({
      args: {
        amount: stakeAmount,
        pool_id: config.poolId,
      },
      gas: GAS_FOR_CALL,
    });

    const nearBalanceAfter = await global.usnAccount.state();
    const usnStakeInfo = await global.poolContract.get_account({
      account_id: config.usnId,
    });

    assert(new BN(usnStakeInfo.staked_balance, 10).eq(new BN(stakeAmount, 10)));
    assert(new BN(nearBalanceBefore.amount, 10).sub(new BN(nearBalanceAfter.amount, 10)).gt(new BN(stakeAmount, 10)));
  });

  it('should fail trying to stake more than account has', async () => {
    await assert.rejects(
      async () => {
        await global.usnContract.stake({
          args: {
            amount: '4000000000000000000000000000', // 400 NEAR
            pool_id: config.poolId,
          },
          gas: GAS_FOR_CALL,
        });
      },
      (err) => {
        assert.match(err.message, /The account doesn't have enough balance/);
        return true;
      }
    );
  });

  it('should fail to withdraw as there is no unstaked balance', async () => {
    await assert.rejects(
      async () => {
        await global.usnContract.withdraw_all({
          args: {
            pool_id: config.poolId,
          },
          gas: GAS_FOR_CALL,
        });
      },
      (err) => {
        assert.match(err.message, /Withdrawal amount should be positive/);
        return true;
      }
    );
  });

  it('should unstake certain amount', async () => {
    const unstakeAmount = ONE_NEAR;
    const usnStakeInfoBefore = await global.poolContract.get_account({
      account_id: config.usnId,
    });

    await global.usnContract.unstake({
      args: {
        amount: unstakeAmount,
        pool_id: config.poolId,
      },
      gas: GAS_FOR_CALL,
    });

    const usnStakeInfoAfter = await global.poolContract.get_account({
      account_id: config.usnId,
    });

    assert(new BN(usnStakeInfoBefore.staked_balance, 10).sub(new BN(unstakeAmount, 10)).eq(new BN(usnStakeInfoAfter.staked_balance, 10)));
    assert(new BN(usnStakeInfoAfter.unstaked_balance, 10).eq(new BN(unstakeAmount, 10)));
  });

  it('should unstake all in case specifying bigger amount', async () => {
    const usnStakeInfoBefore = await global.poolContract.get_account({
      account_id: config.usnId,
    });

    await global.usnContract.unstake({
      args: {
        amount: '4000000000000000000000000000', // 400 NEAR
        pool_id: config.poolId,
      },
      gas: GAS_FOR_CALL,
    });

    const usnStakeInfoAfter = await global.poolContract.get_account({
      account_id: config.usnId,
    });

    assert(new BN(usnStakeInfoAfter.staked_balance, 10).eq(new BN(0)));
    assert(new BN(usnStakeInfoAfter.unstaked_balance, 10)
      .sub(new BN(usnStakeInfoBefore.unstaked_balance, 10))
      .eq(new BN(usnStakeInfoBefore.staked_balance, 10)));
  });

  it('should unstake all', async () => {
    const stakeAmount = "10000000000000000000000000"; // 10 NEAR

    await global.usnContract.stake({
      args: {
        amount: stakeAmount,
        pool_id: config.poolId,
      },
      gas: GAS_FOR_CALL,
    });

    const usnStakeInfoBefore = await global.poolContract.get_account({
      account_id: config.usnId,
    });
    assert(new BN(usnStakeInfoBefore.staked_balance, 10).eq(new BN(stakeAmount, 10)));

    await global.usnContract.unstake_all({
      args: {
        pool_id: config.poolId,
      },
      gas: GAS_FOR_CALL,
    });

    const usnStakeInfoAfter = await global.poolContract.get_account({
      account_id: config.usnId,
    });

    assert(new BN(usnStakeInfoAfter.staked_balance, 10).eq(new BN(0)));
    assert(new BN(usnStakeInfoAfter.unstaked_balance, 10)
      .sub(new BN(usnStakeInfoBefore.unstaked_balance, 10))
      .eq(new BN(usnStakeInfoBefore.staked_balance, 10)));
  });

  it('should forbid withdraw as to delay', async () => {
    await assert.rejects(
      async () => {
        await global.usnContract.withdraw_all({
          args: {
            pool_id: config.poolId,
          },
          gas: GAS_FOR_CALL,
        });
      },
      (err) => {
        assert.match(err.message, /The unstaked balance is not yet available due to unstaking delay/);
        return true;
      }
    );
  });
});

describe('Transfer NEAR', function () {
  this.timeout(15000);

  it('should fail being called not by owner', async () => {
    await assert.rejects(
      async () => {
        await global.aliceContract.transfer_near({
          args: {
            account_id: config.aliceId,
            amount: TEN_NEARS,
          },
          amount: ONE_YOCTO,
          gas: GAS_FOR_CALL,
        });
      },
      (err) => {
        assert.match(err.message, /This method can be called only by owner/);
        return true;
      }
    );
  });

  it('should transfer certain amount of NEAR', async () => {
    const nearAliceBalanceBefore = await global.aliceAccount.state();
    const nearUsnBalanceBefore = await global.usnAccount.state();

    await global.usnContract.transfer_near({
      args: {
        account_id: config.aliceId,
        amount: TEN_NEARS,
      },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    const nearAliceBalanceAfter = await global.aliceAccount.state();
    const nearUsnBalanceAfter = await global.usnAccount.state();

    const aliceBalanceDifference = new BN(nearAliceBalanceAfter.amount)
      .sub(new BN(nearAliceBalanceBefore.amount));
    const usnBalanceDifference = new BN(nearUsnBalanceBefore.amount)
      .sub(new BN(nearUsnBalanceAfter.amount));

    assert(new BN(TEN_NEARS)
      .sub(aliceBalanceDifference)
      .lt(new BN('10000000000000000000000')) // Transfer loss < 0.01 NEAR
    );
    assert(new BN(TEN_NEARS)
      .sub(usnBalanceDifference)
      .lt(new BN('1000000000000000000000')) // Transfer loss < 0.001 NEAR
    );
  });
});