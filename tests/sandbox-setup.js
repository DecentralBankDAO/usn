'use strict';
const nearAPI = require('near-api-js');
const BN = require('bn.js');
const fs = require('fs').promises;
const portUsed = require('port-used');

process.env.NEAR_NO_LOGS = 'defined';

const config = {
  networkId: 'sandbox',
  domain: '0.0.0.0',
  port: 3030,
  keyPath: '/tmp/near-usn-test-sandbox/validator_key.json',
  usnPath: './target/wasm32-unknown-unknown/sandbox/usn.wasm',
  usdtPath: './tests/test_token.wasm',
  refPath: './tests/ref_exchange.wasm',
  wnearPath: './tests/wrap_token.wasm',
  priceoraclePath: './tests/price_oracle.wasm',
  priceoracleMultiplier: '111439',
  amount: new BN('300000000000000000000000000', 10), // 26 digits, 300 NEAR
  masterId: 'test.near',
  usnId: 'usn.test.near',
  usdtId: 'usdt.test.near',
  refId: 'ref.test.near',
  wnearId: 'wrap.test.near',
  oracleId: 'priceoracle.test.near',
  aliceId: 'alice.test.near',
  bobId: 'bob.test.near',
};

const usnMethods = {
  viewMethods: [
    'version',
    'name',
    'symbol',
    'decimals',
    'spread',
    'contract_status',
    'owner',
    'ft_balance_of',
    'storage_balance_of',
    'commission',
    'guardians',
    'treasury',
    "history",
    'predict_buy',
    'predict_sell',
  ],
  changeMethods: [
    'new',
    'upgrade_name_symbol',
    'upgrade_icon',
    'blacklist_status',
    'add_to_blacklist',
    'remove_from_blacklist',
    'propose_new_owner',
    'accept_ownership',
    'set_fixed_spread',
    'set_adaptive_spread',
    'extend_guardians',
    'remove_guardians',
    'destroy_black_funds',
    'pause',
    'resume',
    'buy',
    'sell',
    'ft_transfer',
    'ft_transfer_call',
    'transfer_stable_liquidity',
    'balance_stable_pool',
    'balance_treasury',
    'warmup',
    'refund_treasury'
  ],
};

const oracleMethods = {
  changeMethods: [
    'new',
    'add_asset',
    'add_asset_ema',
    'add_oracle',
    'report_prices',
  ],
};

const usdtMethods = {
  viewMethods: ['ft_balance_of'],
  changeMethods: ['new', 'mint', 'burn', 'ft_transfer', 'ft_transfer_call'],
};

const refMethods = {
  viewMethods: ['get_stable_pool', 'get_pool_shares', 'get_deposit'],
  changeMethods: [
    'new',
    'storage_deposit',
    'add_stable_swap_pool',
    'extend_whitelisted_tokens',
    'add_stable_liquidity',
    'add_simple_pool',
    'add_liquidity'
  ],
};

const wnearMethods = {
  viewMethods: ['ft_balance_of'],
  changeMethods: ['new', 'mint', 'burn', 'near_deposit', 'ft_transfer', 'near_withdraw', 'ft_transfer_call',],
};

async function sandboxSetup() {
  portUsed.check(config.port, config.domain)
    .then((inUse) => {
      if (!inUse) {
        throw new Error('Run sandbox first: `npm run sandbox:test`!');
      }
    }, (err) => {
      console.error('Error on check:', err.message);
    });

  const keyFile = require(config.keyPath);
  const privKey = nearAPI.utils.KeyPair.fromString(keyFile.secret_key);
  const pubKey = nearAPI.utils.PublicKey.fromString(keyFile.public_key);

  const keyStore = new nearAPI.keyStores.InMemoryKeyStore();
  keyStore.setKey(config.networkId, config.masterId, privKey);

  const near = await nearAPI.connect({
    deps: {
      keyStore,
    },
    networkId: config.networkId,
    nodeUrl: 'http://' + config.domain + ':' + config.port,
  });

  // Setup a global test context before anything else failed.
  global.near = near;

  let masterAccount = new nearAPI.Account(near.connection, config.masterId);

  // Create test accounts.
  await masterAccount.createAccount(config.usnId, pubKey, config.amount);
  await masterAccount.createAccount(config.usdtId, pubKey, config.amount);
  await masterAccount.createAccount(config.refId, pubKey, config.amount);
  await masterAccount.createAccount(config.wnearId, pubKey, config.amount);
  await masterAccount.createAccount(config.oracleId, pubKey, config.amount);
  await masterAccount.createAccount(config.aliceId, pubKey, config.amount);
  await masterAccount.createAccount(config.bobId, pubKey, config.amount);
  keyStore.setKey(config.networkId, config.usnId, privKey);
  keyStore.setKey(config.networkId, config.usdtId, privKey);
  keyStore.setKey(config.networkId, config.refId, privKey);
  keyStore.setKey(config.networkId, config.wnearId, privKey);
  keyStore.setKey(config.networkId, config.oracleId, privKey);
  keyStore.setKey(config.networkId, config.aliceId, privKey);
  keyStore.setKey(config.networkId, config.bobId, privKey);

  // Deploy the USN contract.
  const wasm = await fs.readFile(config.usnPath);
  const usnAccount = new nearAPI.Account(near.connection, config.usnId);
  await usnAccount.deployContract(wasm);

  // Initialize the contract.
  const usnContract = new nearAPI.Contract(
    usnAccount,
    config.usnId,
    usnMethods
  );
  await usnContract.new({ args: { owner_id: config.usnId } });

  // Deploy USDT contract.
  const wasmUsdt = await fs.readFile(config.usdtPath);
  const usdtAccount = new nearAPI.Account(near.connection, config.usdtId);
  await usdtAccount.deployContract(wasmUsdt);

  // Initialize USDT contract.
  const usdtContract = new nearAPI.Contract(
    usdtAccount,
    config.usdtId,
    usdtMethods
  );
  await usdtContract.new({ args: {} });
  // Register accounts in USDT contract to enable depositing.
  await usdtContract.mint({
    args: { account_id: config.usdtId, amount: '10000000000000' },
  });
  await usdtContract.mint({
    args: { account_id: config.refId, amount: '0' },
  });
  await usdtContract.mint({
    args: { account_id: config.usnId, amount: '10000000000000' },
  });

  // Deploy WNEAR contract.
  const wasmWnear = await fs.readFile(config.wnearPath);
  const wnearAccount = new nearAPI.Account(near.connection, config.wnearId);
  await wnearAccount.deployContract(wasmWnear);

  // Initialize WNEAR contract.
  const wnearContract = new nearAPI.Contract(
    wnearAccount,
    config.wnearId,
    wnearMethods
  );

  await wnearContract.new({ args: {} });
  // Register accounts in WNEAR contract to enable depositing.
  await wnearContract.mint({
    args: { account_id: config.wnearId, amount: '1000000000000000000000000' },
  });
  await wnearContract.mint({
    args: { account_id: config.refId, amount: '0' },
  });
  await wnearContract.mint({
    args: { account_id: config.usnId, amount: '1000000000000000000000000000' },
  });

  // Deploy Ref.Finance (ref-exchange) contract.
  const wasmRef = await fs.readFile(config.refPath);
  const refAccount = new nearAPI.Account(near.connection, config.refId);
  await refAccount.deployContract(wasmRef);

  // Initialize Ref.Finance contract.
  const refContract = new nearAPI.Contract(
    refAccount,
    config.refId,
    refMethods
  );
  await refContract.new({
    args: { owner_id: config.refId, exchange_fee: 1600, referral_fee: 400 },
  });

  const usnRef = new nearAPI.Contract(usnAccount, config.refId, refMethods);
  await usnRef.storage_deposit({ args: {}, amount: '10000000000000000000000' });

  // pool_id: 0
  await refContract.add_stable_swap_pool({
    args: {
      tokens: [config.usnId, config.usdtId],
      decimals: [18, 6],
      fee: 25,
      amp_factor: 240,
    },
    amount: '3540000000000000000000',
  });
  // pool_id: 1
  await refContract.add_stable_swap_pool({
    args: {
      tokens: [config.usnId, config.usdtId],
      decimals: [18, 6],
      fee: 25,
      amp_factor: 240,
    },
    amount: '3540000000000000000000',
  });
  // pool_id: 2
  await refContract.add_simple_pool({
    args: {
      tokens: [config.wnearId, config.usdtId],
      fee: 25,
    },
    amount: '3550000000000000000000',
  });

  await refContract.extend_whitelisted_tokens({
    args: {
      tokens: [config.usdtId, config.wnearId, config.usnId]
    },
    amount: '1',
  });

  // Deploy the priceoracle contract.
  const wasmPriceoracle = await fs.readFile(config.priceoraclePath);
  const oracleAccount = new nearAPI.Account(near.connection, config.oracleId);
  await oracleAccount.deployContract(wasmPriceoracle);

  // Initialize the Oracle contract.
  const oracleContract = new nearAPI.Contract(
    oracleAccount,
    config.oracleId,
    oracleMethods
  );
  await oracleContract.new({
    args: {
      recency_duration_sec: 360,
      owner_id: config.oracleId,
      near_claim_amount: '0',
    },
  });
  await oracleContract.add_oracle({
    args: { account_id: config.oracleId },
    amount: '1',
  });
  await oracleContract.add_asset({
    args: { asset_id: 'wrap.test.near' },
    amount: '1',
  });
  await oracleContract.add_asset_ema({
    args: { asset_id: 'wrap.test.near', period_sec: 3600 },
    amount: '1',
  });
  await oracleContract.report_prices({
    args: {
      prices: [
        {
          asset_id: 'wrap.test.near',
          price: { multiplier: config.priceoracleMultiplier, decimals: 28 },
        },
      ],
    },
  });

  // Initialize other accounts connected to the contract for all test cases.
  const aliceAccount = new nearAPI.Account(near.connection, config.aliceId);
  const bobAccount = new nearAPI.Account(near.connection, config.bobId);
  const aliceContract = new nearAPI.Contract(
    aliceAccount,
    config.usnId,
    usnMethods
  );
  const bobContract = new nearAPI.Contract(
    bobAccount,
    config.usnId,
    usnMethods
  );
  const bobUsdt = new nearAPI.Contract(bobAccount, config.usdtId, usdtMethods);
  const usnUsdt = new nearAPI.Contract(usnAccount, config.usdtId, usdtMethods);
  const usnWnear = new nearAPI.Contract(usnAccount, config.wnearId, wnearMethods);

  // Setup a global test context.
  global.usnAccount = usnAccount;
  global.usnContract = usnContract;
  global.usdtContract = usdtContract;
  global.refContract = refContract;
  global.wnearContract = wnearContract;
  global.priceoracleContract = oracleContract;
  global.aliceAccount = aliceAccount;
  global.aliceContract = aliceContract;
  global.bobContract = bobContract;
  global.bobUsdt = bobUsdt;
  global.usnUsdt = usnUsdt;
  global.usnWnear = usnWnear;
  global.usnRef = usnRef;
}

async function sandboxTeardown() {
  const near = global.near;

  const alice = new nearAPI.Account(near.connection, config.aliceId);
  const bob = new nearAPI.Account(near.connection, config.bobId);
  const usn = new nearAPI.Account(near.connection, config.usnId);
  const oracle = new nearAPI.Account(near.connection, config.oracleId);

  await alice.deleteAccount(config.masterId);
  await bob.deleteAccount(config.masterId);
  await usn.deleteAccount(config.masterId);
  await oracle.deleteAccount(config.masterId);
}

module.exports = { config, sandboxSetup, sandboxTeardown };

module.exports.mochaHooks = {
  beforeAll: async function () {
    this.timeout(80000);
    await sandboxSetup();
  },
  afterAll: async function () {
    this.timeout(10000);
    await sandboxTeardown();
  },
};
