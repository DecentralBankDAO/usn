'use strict';

const assert = require('assert').strict;
const config = require('./sandbox-setup').config;
const BN = require('bn.js');

const ONE_NEAR = '1000000000000000000000000';
const ONE_YOCTO = '1';
const HUNDRED_NEARS = '100000000000000000000000000';
const GAS_FOR_CALL = '200000000000000'; // 200 TGas

describe('Smoke Test', function () {
  it('should get a version', async () => {
    const version = await global.aliceContract.version();
    assert.match(version, /1\..\../);
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

  it('should get a spread', async () => {
    const spread = await global.aliceContract.spread();
    assert.equal(spread, '5000');
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
    assert.deepEqual(commission, {
      near: '0',
      usn: '0',
    });
  });

  it('should get a treasury', async () => {
    const treasury = await global.aliceContract.treasury();
    assert.deepEqual(treasury, {
      reserve: {},
      cache: {
        items: [],
      },
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
  this.timeout(5000);

  before(async () => {
    await global.usnContract.set_owner({
      args: { owner_id: config.aliceId },
    });
    assert.equal(await global.usnContract.owner(), config.aliceId);
  });

  it('can change ownership', async () => {
    await assert.rejects(async () => {
      await global.usnContract.set_owner({ args: { owner_id: config.usnId } });
    });
  });

  after(async () => {
    await global.aliceContract.set_owner({
      args: { owner_id: config.usnId },
    });
    assert.equal(await global.aliceContract.owner(), config.usnId);
  });
});

describe('Guardian', function () {
  this.timeout(5000);

  before(async () => {
    await global.usnContract.extend_guardians({
      args: { guardians: [config.aliceId] },
    });
  });

  it('should be able to pause the contract', async () => {
    assert.doesNotReject(async () => {
      await global.aliceContract.pause({ args: {} });
      assert.equal(await global.aliceContract.contract_status(), 'Paused');
    });

    await assert.rejects(async () => {
      await global.aliceContract.ft_transfer({
        args: { receiver_id: 'any', amount: '1' },
      });
    });
  });

  after(async () => {
    await global.usnContract.remove_guardians({
      args: { guardians: [config.aliceId] },
    });
  });
});

describe('User', async function () {
  this.timeout(15000);

  it('should NOT sell before buying', async () => {
    await assert.rejects(async () => {
      await global.aliceContract.sell({ args: { amount: 1 } });
    });
  });

  it('should buy USN to get registered', async () => {
    const amount = await global.aliceContract.buy({
      args: {},
      amount: ONE_NEAR,
      gas: GAS_FOR_CALL,
    });
    assert.equal(amount, '11088180500000000000'); // no storage fee

    const expected_amount = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId,
    });
    assert.equal(amount, expected_amount);
  });

  it('can buy USN with the expected rate', async () => {
    const amount = await global.aliceContract.buy({
      args: {
        expected: { multiplier: '111439', slippage: '10', decimals: 28 },
      },
      amount: ONE_NEAR,
      gas: GAS_FOR_CALL,
    });
    assert.equal(amount, '11088180500000000000');
  });

  it('should NOT register the recipient having not enough money to buy USN', async () => {
    await assert.rejects(
      async () => {
        await global.aliceContract.buy({
          args: {
            expected: { multiplier: '111439', slippage: '10', decimals: 28 },
            to: config.bobId,
          },
          amount: ONE_YOCTO, // very small attached deposit
          gas: GAS_FOR_CALL,
        });
      },
      (err) => {
        assert.match(err.message, /attached deposit exchanges to 0 tokens/);
        return true;
      }
    );
  });

  it('can buy USN for unregistered user (the recipient gets auto-registered)', async () => {
    const amount = await global.aliceContract.buy({
      args: {
        to: config.bobId,
      },
      amount: ONE_NEAR,
      gas: GAS_FOR_CALL,
    });
    assert.equal(amount, '11088180500000000000'); // no storage fee

    const expected_amount = await global.bobContract.ft_balance_of({
      account_id: config.bobId,
    });
    assert.equal(amount, expected_amount);
  });

  it('can NOT buy with slippage control in place', async () => {
    await assert.rejects(
      async () => {
        await global.aliceContract.buy({
          args: {
            expected: { multiplier: '111428', slippage: '10', decimals: 28 },
          },
          amount: ONE_NEAR,
          gas: GAS_FOR_CALL,
        });
      },
      (err) => {
        assert.match(err.message, /Slippage error/);
        return true;
      }
    );
  });

  it('sells USN with the current exchange rate', async () => {
    const near = await global.aliceContract.sell({
      args: {
        amount: '11032461000000000000',
      },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });
    assert.equal(near, '985050000000000000000000'); // 0.98 NEAR
  });

  it('sells USN with slippage control', async () => {
    const near = await global.bobContract.sell({
      args: {
        amount: '11032461000000000000',
      },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });
    assert.equal(near, '985050000000000000000000'); // 0.97 NEAR
  });

  it('spends gas and gets the rest back in case of error', async () => {
    const balance = (await global.aliceContract.account.getAccountBalance())
      .available;
    await assert.rejects(async () => {
      await global.aliceContract.buy({
        args: {
          expected: { multiplier: '111428', slippage: '10', decimals: 28 },
        },
        amount: ONE_NEAR,
        gas: GAS_FOR_CALL,
      });
    });
    const balance2 = (await global.aliceContract.account.getAccountBalance())
      .available;
    assert.equal(balance.length, balance2.length);
    // 9.99 NEAR -> 9.97 NEAR
    // 5.71 NEAR -> 5.68 NEAR
    const near_before = parseInt(balance.substring(0, 3));
    const near_after = parseInt(balance2.substring(0, 3));
    // Should be less than 3-4, but it's 6 (0.06, ~$0.6) because of the sandbox issue.
    assert(near_before - near_after < 6);
  });

  it('should sell all USN to get unregistered', async () => {
    await global.aliceContract.sell({
      args: {
        amount: await global.aliceContract.ft_balance_of({
          account_id: config.aliceId,
        }),
      },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    assert.equal(
      await global.aliceContract.ft_balance_of({
        account_id: config.aliceId,
      }),
      '0'
    );

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

    await global.bobContract.sell({
      args: {
        amount: await global.bobContract.ft_balance_of({
          account_id: config.bobId,
        }),
      },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });
  });

  after(async () => {
    const aliceBalance = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId,
    });

    const bobBalance = await global.bobContract.ft_balance_of({
      account_id: config.bobId,
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

    if (bobBalance != '0') {
      await global.bobContract.ft_transfer({
        args: {
          receiver_id: 'any',
          amount: bobBalance,
        },
        amount: ONE_YOCTO,
      });
    }
  });
});

describe('Adaptive Spread', async function () {
  this.timeout(15000);

  it('should be used to buy USN', async () => {
    const amount = await global.aliceContract.buy({
      args: {},
      amount: HUNDRED_NEARS,
      gas: GAS_FOR_CALL,
    });
    assert.equal(amount, '1108854824870000000000'); // ~$1108

    const expected_amount = await global.aliceContract.ft_balance_of({
      account_id: config.aliceId,
    });
    assert.equal(amount, expected_amount);
  });

  it('should be used to sell USN', async () => {
    const near = await global.aliceContract.sell({
      args: {
        amount: '1108854824870000000000',
      },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });
    assert.equal(near, '99009067108900000000000000'); // 0.99 NEAR
  });

  it('should be configurable', async () => {
    await global.usnContract.set_adaptive_spread({
      args: { params: { min: 0.002, max: 0.006, scaler: 0.0001 } },
    });

    const amount = await global.aliceContract.buy({
      args: {},
      amount: ONE_NEAR,
      gas: GAS_FOR_CALL,
    });
    assert.equal(amount, '11077081175600000000'); // ~$11.08
  });

  it('should be in limits', async () => {
    // min <= max
    await assert.rejects(async () => {
      await global.usnContract.set_adaptive_spread({
        args: { params: { min: 0.006, max: 0.002, scaler: 0.0001 } },
      });
    });

    // min < 0.05
    await assert.rejects(async () => {
      await global.usnContract.set_adaptive_spread({
        args: { params: { min: 0.06, max: 0.01, scaler: 0.0001 } },
      });
    });

    // max < 0.05
    await assert.rejects(async () => {
      await global.usnContract.set_adaptive_spread({
        args: { params: { min: 0.01, max: 0.06, scaler: 0.0001 } },
      });
    });

    // scaler < 0.4
    await assert.rejects(async () => {
      await global.usnContract.set_adaptive_spread({
        args: { params: { min: 0.01, max: 0.03, scaler: 0.5 } },
      });
    });

    // only positive
    await assert.rejects(async () => {
      await global.usnContract.set_adaptive_spread({
        args: { params: { min: 0.001, max: 0.003, scaler: -0.4 } },
      });
    });
  });
});

describe('Fixed Spread', async function () {
  this.timeout(15000);

  before(async () => {
    await global.usnContract.set_fixed_spread({ args: { spread: '10000' } }); // 1%
  });

  it('should be used to buy USN', async () => {
    const amount = await global.aliceContract.buy({
      args: {},
      amount: HUNDRED_NEARS,
      gas: GAS_FOR_CALL,
    });
    assert.equal(amount, '1103246100000000000000'); // ~$1103
  });

  it('should be used to sell USN', async () => {
    const near = await global.aliceContract.sell({
      args: {
        amount: '1103246100000000000000',
      },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });
    assert.equal(near, '98010000000000000000000000'); // 98.01 NEAR
  });

  after(async () => {
    await global.usnContract.set_adaptive_spread({ args: {} });
  });
});

describe('Stable Pool (USDT/USN) [pool_id: 0]', async function () {
  this.timeout(19000);

  const MAX_TRANSFER_COST = '780000000000000000001';

  var dao;

  before(async () => {
    await global.usnContract.set_owner({
      args: { owner_id: config.aliceId },
    });
    dao = global.aliceContract;
  });

  beforeEach(async () => {
    await global.usdtContract.burn({
      args: {
        account_id: config.usnId,
        amount: await global.usdtContract.ft_balance_of({
          account_id: config.usnId,
        }),
      },
    });
  });

  it('should finalize depositing after failure', async () => {
    await global.usdtContract.ft_transfer({
      args: { receiver_id: config.usnId, amount: '1000000000000' },
      amount: '1',
    });

    // Should fail at the `add_stable_liquidity` cross-contract call.
    // But deposit already belongs to the ref.finance account.
    await assert.rejects(
      async () => {
        await dao.transfer_stable_liquidity({
          args: { pool_id: 0, whole_amount: '1000000' },
          amount: '2',
          gas: GAS_FOR_CALL,
        });
      },
      (err) => {
        assert.match(
          err.message,
          /ERR_STORAGE_DEPOSIT need 780000000000000000000/
        );
        return true;
      }
    );

    const refUsdt = await global.usdtContract.ft_balance_of({
      account_id: config.refId,
    });
    const refUsn = await global.usnContract.ft_balance_of({
      account_id: config.refId,
    });

    await assert.notEqual(refUsdt, '0');
    await assert.notEqual(refUsn, '0');
    await assert.equal(
      await global.usdtContract.ft_balance_of({ account_id: config.usnId }),
      '0'
    );

    const poolInfo = await global.refContract.get_stable_pool({ pool_id: 0 });

    await assert.doesNotReject(async () => {
      const shares = await dao.transfer_stable_liquidity({
        args: { pool_id: 0, whole_amount: '1000000' },
        amount: MAX_TRANSFER_COST,
        gas: GAS_FOR_CALL,
      });

      assert.equal(shares, '2000000000000000000000000');
    });

    const refUsdt2 = await global.usdtContract.ft_balance_of({
      account_id: config.refId,
    });
    const refUsn2 = await global.usnContract.ft_balance_of({
      account_id: config.refId,
    });

    // The second call of transfer_stable_liquidity should not mint additional money.
    await assert.equal(refUsdt, refUsdt2);
    await assert.equal(refUsn, refUsn2);

    const poolInfo2 = await global.refContract.get_stable_pool({ pool_id: 0 });
    assert.notDeepEqual(poolInfo.amounts, poolInfo2.amounts);

    // Pool must grow exactly on $1000000.
    assert(
      new BN('1000000000000', 10).eq(
        new BN(poolInfo2.amounts[1], 10).sub(new BN(poolInfo.amounts[1], 10))
      )
    );
  });

  it('should fail having not enough USDT', async () => {
    // Should fail after the 1st ft_transfer_call.
    await assert.rejects(
      async () => {
        await dao.transfer_stable_liquidity({
          args: { pool_id: 0, whole_amount: '1000000' }, // $1 mln.
          amount: MAX_TRANSFER_COST,
          gas: GAS_FOR_CALL,
        });
      },
      (err) => {
        assert.match(err.message, /Not enough usdt.test.near/);
        return true;
      }
    );
  });

  it('should fail having not enough attached NEAR', async () => {
    await assert.rejects(
      async () => {
        await dao.transfer_stable_liquidity({
          args: { pool_id: 0, whole_amount: '1000000' },
          amount: '1',
          gas: GAS_FOR_CALL,
        });
      },
      (err) => {
        assert.match(
          err.message,
          /Requires attached deposit more than 1 yoctoNEAR/
        );
        return true;
      }
    );
  });

  after(async () => {
    await dao.set_owner({
      args: { owner_id: config.usnId },
    });
  });
});

describe('Stable Pool (USDT/USN) [pool_id: 1]', async function () {
  this.timeout(20000);

  const MAX_TRANSFER_COST = '780000000000000000001';

  var dao;

  before(async () => {
    // Set up "DAO" account.
    await global.usnContract.set_owner({
      args: { owner_id: config.aliceId },
    });
    dao = global.aliceContract;

    // Fill up USN account with the USDT token: $1000000.
    await global.usdtContract.ft_transfer({
      args: { receiver_id: config.usnId, amount: '1000000000000' },
      amount: '1',
    });

    // Register Bob in the USDT contract.
    // Otherwise, ref.finance won't finish a swap.
    await usdtContract.mint({
      args: { account_id: config.bobId, amount: '0' },
    });

    // Add stable liquidity to the stable pool.
    await dao.transfer_stable_liquidity({
      args: { pool_id: 1, whole_amount: '1000000' },
      amount: MAX_TRANSFER_COST,
      gas: GAS_FOR_CALL,
    });
  });

  it('should NOT be balanced when USDT < USN', async () => {
    // Bob buys USN.
    const amount = await global.bobContract.buy({
      args: {},
      amount: ONE_NEAR,
      gas: GAS_FOR_CALL,
    });

    // Bob swaps USN to USDT: BOB -> USN -> REF + ACTION.
    await global.bobContract.ft_transfer_call({
      args: {
        receiver_id: config.refId,
        amount: amount,
        msg: '{"actions": [{"pool_id": 1, "token_in": "usn.test.near", "token_out": "usdt.test.near", "min_amount_out": "1"}]}',
      },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    // Now, Bob has some USDT.
    assert.notEqual(
      '0',
      await global.usdtContract.ft_balance_of({ account_id: config.bobId })
    );

    // And the pool is unbalanced.
    const poolInfo = await global.refContract.get_stable_pool({ pool_id: 1 });
    assert(
      new BN(poolInfo.amounts[1] + '000000000000', 10).lt(
        new BN(poolInfo.amounts[0], 10)
      )
    );

    // Balancing the pool now.
    await dao.balance_stable_pool({
      args: { pool_id: 1 },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    // Nothing should happen.
    const poolInfo2 = await global.refContract.get_stable_pool({ pool_id: 1 });
    assert(
      new BN(poolInfo.amounts[1] + '000000000000', 10).lt(
        new BN(poolInfo.amounts[0], 10)
      )
    );
  });

  it('should be balanced when USDT > USN', async () => {
    // Bob buys USDT.
    await global.usdtContract.ft_transfer({
      args: { receiver_id: config.bobId, amount: '1000000000' },
      amount: '1',
    });

    // Bob swaps USDT to USN: BOB -> USDT -> REF + ACTION.
    await global.bobUsdt.ft_transfer_call({
      args: {
        receiver_id: config.refId,
        amount: '1000000000',
        msg: '{"actions": [{"pool_id": 1, "token_in": "usdt.test.near", "token_out": "usn.test.near", "min_amount_out": "1"}]}',
      },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    // The pool is unbalanced.
    const poolInfo = await global.refContract.get_stable_pool({ pool_id: 1 });
    assert(
      new BN(poolInfo.amounts[1] + '000000000000', 10).gt(
        new BN(poolInfo.c_amounts[0], 10)
      )
    );

    // Balancing the pool now.
    await dao.balance_stable_pool({
      args: { pool_id: 1 },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    const poolInfo2 = await global.refContract.get_stable_pool({ pool_id: 1 });
    assert.equal(poolInfo2.amounts[1] + '000000000000', poolInfo2.amounts[0]);
  });
  after(async () => {
    await dao.set_owner({
      args: { owner_id: config.usnId },
    });
  });
});

describe('Balance treasury', async function () {
  this.timeout(50000);

  const MAX_TRANSFER_COST = '780000000000000000001';

  before(async () => {
    // Fill up USN account with the USDT token: $1000000.
    await global.usdtContract.ft_transfer({
      args: { receiver_id: config.usnId, amount: '1000000000000' },
      amount: '1',
    });

    // Add stable liquidity to the stable pool.
    await global.usnContract.transfer_stable_liquidity({
      args: { pool_id: 1, whole_amount: '1000000' },
      amount: MAX_TRANSFER_COST,
      gas: GAS_FOR_CALL,
    });

    await global.usdtContract.ft_transfer({
      args: { receiver_id: config.usnId, amount: '10000000000' },
      amount: '1',
    });

    await global.usnUsdt.ft_transfer_call({
      args: {
        receiver_id: config.refId,
        amount: '10000000000',
        msg: '',
      },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    await global.usnWnear.ft_transfer_call({
      args: {
        receiver_id: config.refId,
        amount: '1000000000000000000000000000',
        msg: '',
      },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    // Add liquidity to uniswap pool NEAR-USDT
    await global.usnRef.add_liquidity({
      args: {
        pool_id: 2,
        amounts: ['1000000000000000000000000000', '10000000000'],
        min_shares: '0',
      },
      amount: '780000000000000000000',
    });

    // Warmup
    await global.usnContract.warmup({
      args: {},
      gas: GAS_FOR_CALL,
    });
  });

  it('should be balanced by itself', async () => {
    const poolShareBefore = await global.refContract.get_pool_shares({
      pool_id: 1,
      account_id: config.usnId,
    });
    assert.equal(poolShareBefore, '4001968963282490744611320');

    // Balancing the treasury
    await global.usnContract.balance_treasury({
      args: {
        pool_id: 1,
        limits: [1000, 2000],
        execute: true,
      },
      amount: 3 * ONE_YOCTO,
      gas: '300000000000000',
    });

    const poolShareAfter = await global.refContract.get_pool_shares({
      pool_id: 1,
      account_id: config.usnId,
    });
    assert(new BN(poolShareAfter, 10).lt(new BN(poolShareBefore, 10)));
  });
});

describe('Refund treasury', async function () {
  this.timeout(30000);

  const MAX_TRANSFER_COST = '780000000000000000001';

  before(async () => {
    // Fill up USN account with the USDT token: $1000000.
    await global.usdtContract.ft_transfer({
      args: { receiver_id: config.usnId, amount: '1000000000000' },
      amount: '1',
    });

    // Add stable liquidity to the stable pool.
    await global.usnContract.transfer_stable_liquidity({
      args: { pool_id: 0, whole_amount: '1000000' },
      amount: MAX_TRANSFER_COST,
      gas: GAS_FOR_CALL,
    });
  });

  it('should fail being called not by owner or guardian', async () => {
    await assert.rejects(
      async () => {
        await global.aliceContract.refund_treasury({
          args: {},
          gas: GAS_FOR_CALL,
        });
      },
      (err) => {
        assert.match(
          err.message,
          /This method can be called only by owner or guardian/
        );
        return true;
      }
    );
  });

  it('should handle refund', async () => {
    // 'usn' account buys some $USN for itself.
    const usnAmount = await global.usnContract.buy({
      args: {},
      amount: ONE_NEAR,
      gas: GAS_FOR_CALL,
    });

    // Deposit $USN on ref.finance.
    await global.usnContract.ft_transfer_call({
      args: {
        receiver_id: config.refId,
        amount: usnAmount,
        msg: '',
      },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    await global.usdtContract.ft_transfer({
      args: { receiver_id: config.usnId, amount: '1000' },
      amount: '1',
    });

    // Deposit $USDT.
    await global.usnUsdt.ft_transfer_call({
      args: {
        receiver_id: config.refId,
        amount: '1000',
        msg: '',
      },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    // Clear wNEAR 'usn' account.
    await global.wnearContract.burn({
      args: {
        account_id: config.usnId,
        amount: await global.wnearContract.ft_balance_of({
          account_id: config.usnId,
        }),
      },
      gas: GAS_FOR_CALL,
    });

    // "Mint" 0.1 wNEAR for 'usn' account.
    await global.wnearContract.ft_transfer({
      args: { receiver_id: config.usnId, amount: '100000000000000000000000' },
      amount: ONE_YOCTO,
    });

    // Deposit wNEAR.
    await global.usnWnear.ft_transfer_call({
      args: {
        receiver_id: config.refId,
        amount: '50000000000000000000000',
        msg: '',
      },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    const wrapAmountBefore = await global.wnearContract.ft_balance_of({
      account_id: config.usnId,
    });

    assert.equal(wrapAmountBefore, '50000000000000000000000');

    const poolInfoBefore = await global.refContract.get_stable_pool({
      pool_id: 0,
    });

    const nearAmountBefore = await global.usnAccount.state();

    await global.usnContract.refund_treasury({
      args: {},
      amount: 3 * ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    const nearAmountAfter = await global.usnAccount.state();

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
    const wrapDeposit = await global.refContract.get_deposit({
      account_id: config.usnId,
      token_id: config.wnearId,
    });
    const wrapAmount = await global.wnearContract.ft_balance_of({
      account_id: config.usnId,
    });

    assert.equal(usnDeposit, '0');
    assert.equal(usdtDeposit, '0');
    assert.equal(wrapDeposit, '0');
    assert.equal(wrapAmount, '0');

    // The result is less than 0.1 $NEAR because of gas spent
    assert(
      new BN(nearAmountAfter.amount, 10)
        .sub(new BN(nearAmountBefore.amount, 10))
        .lt(new BN('100000000000000000000000', 10))
    );
    // But greater than 0.03 $NEAR.
    assert(
      new BN(nearAmountAfter.amount, 10)
        .sub(new BN(nearAmountBefore.amount, 10))
        .gt(new BN('030000000000000000000000', 10))
    );

    // Stable pool has been filled with liquidity.
    assert(
      new BN(poolInfoBefore.amounts[0], 10).lt(
        new BN(poolInfoAfter.amounts[0], 10)
      )
    );
    assert(
      new BN(poolInfoBefore.amounts[1], 10).lt(
        new BN(poolInfoAfter.amounts[1], 10)
      )
    );
  });

  it('should handle refund without usdt deposit', async () => {
    await global.wnearContract.ft_transfer({
      args: { receiver_id: config.usnId, amount: '100000000000000000000000' },
      amount: ONE_YOCTO,
    });

    await global.usnWnear.ft_transfer_call({
      args: {
        receiver_id: config.refId,
        amount: '100000000000',
        msg: '',
      },
      amount: ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    const usdtDepositBefore = await global.refContract.get_deposit({
      account_id: config.usnId,
      token_id: config.usdtId,
    });
    const wnearDepositBefore = await global.refContract.get_deposit({
      account_id: config.usnId,
      token_id: config.wnearId,
    });
    assert.equal(usdtDepositBefore, '0');
    assert.notEqual(wnearDepositBefore, '0');

    const wrapAmountBefore = await global.wnearContract.ft_balance_of({
      account_id: config.usnId,
    });
    assert(new BN(wrapAmountBefore, 10).gt(new BN('0', 10)));
    const nearAmountBefore = await global.usnAccount.state();

    await global.usnContract.refund_treasury({
      args: {},
      amount: 3 * ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });
    const nearAmountAfter = await global.usnAccount.state();

    const usdtDeposit = await global.refContract.get_deposit({
      account_id: config.usnId,
      token_id: config.usdtId,
    });
    const wrapDeposit = await global.refContract.get_deposit({
      account_id: config.usnId,
      token_id: config.wnearId,
    });
    const wrapAmount = await global.wnearContract.ft_balance_of({
      account_id: config.usnId,
    });

    assert.equal(usdtDeposit, '0');
    assert.equal(wrapDeposit, '0');
    assert.equal(wrapAmount, '0');
    // The result is less because of gas spent
    assert(
      new BN(nearAmountAfter.amount, 10)
        .sub(new BN(nearAmountBefore.amount, 10))
        .lt(new BN('100000000000000000000000', 10))
    );
  });

  it('should handle refund without any deposit', async () => {
    const usnDepositBefore = await global.refContract.get_deposit({
      account_id: config.usnId,
      token_id: config.usnId,
    });
    const usdtDepositBefore = await global.refContract.get_deposit({
      account_id: config.usnId,
      token_id: config.usdtId,
    });
    const wnearDepositBefore = await global.refContract.get_deposit({
      account_id: config.usnId,
      token_id: config.wnearId,
    });
    assert.equal(usnDepositBefore, '0');
    assert.equal(usdtDepositBefore, '0');
    assert.equal(wnearDepositBefore, '0');

    await global.wnearContract.ft_transfer({
      args: { receiver_id: config.usnId, amount: '100000000000000000000000' },
      amount: ONE_YOCTO,
    });

    const nearAmountBefore = await global.usnAccount.state();

    await global.usnContract.refund_treasury({
      args: {},
      amount: 3 * ONE_YOCTO,
      gas: GAS_FOR_CALL,
    });

    const nearAmountAfter = await global.usnAccount.state();

    const usnDeposit = await global.refContract.get_deposit({
      account_id: config.usnId,
      token_id: config.usnId,
    });
    const usdtDeposit = await global.refContract.get_deposit({
      account_id: config.usnId,
      token_id: config.usdtId,
    });
    const wrapDeposit = await global.refContract.get_deposit({
      account_id: config.usnId,
      token_id: config.wnearId,
    });
    const wrapAmount = await global.wnearContract.ft_balance_of({
      account_id: config.usnId,
    });

    assert.equal(usnDeposit, '0');
    assert.equal(usdtDeposit, '0');
    assert.equal(wrapDeposit, '0');
    assert.equal(wrapAmount, '0');

    // The result is less than 0.1 $NEAR because of gas spent
    assert(
      new BN(nearAmountAfter.amount, 10)
        .sub(new BN(nearAmountBefore.amount, 10))
        .lt(new BN('100000000000000000000000', 10))
    );
    // But greater than 0.06 $NEAR.
    assert(
      new BN(nearAmountAfter.amount, 10)
        .sub(new BN(nearAmountBefore.amount, 10))
        .gt(new BN('060000000000000000000000', 10))
    );
  });
});
