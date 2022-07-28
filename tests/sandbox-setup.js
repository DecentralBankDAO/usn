'use strict';
const nearAPI = require('near-api-js');
const BN = require('bn.js');
const fs = require('fs').promises;
const portUsed = require('port-used');

process.env.NEAR_NO_LOGS = 'defined';

const port = process.env.SANDBOX_PORT || 3030;

const config = {
  networkId: 'sandbox',
  domain: '0.0.0.0',
  port: port,
  keyPath: '/tmp/near-usn-test-sandbox/validator_key.json',
  usnPath: './target/wasm32-unknown-unknown/sandbox/usn.wasm',
  usdtPath: './tests/test_token.wasm',
  refPath: './tests/ref_exchange.wasm',
  amount: new BN('300000000000000000000000000', 10), // 26 digits, 300 NEAR
  masterId: 'test.near',
  usnId: 'usn.test.near',
  usdtId: 'usdt.test.near',
  refId: 'ref.test.near',
  aliceId: 'alice.test.near',
  carolId: 'carol.test.near',
};

const usnMethods = {
  viewMethods: [
    'version',
    'name',
    'symbol',
    'decimals',
    'contract_status',
    'owner',
    'ft_balance_of',
    'storage_balance_of',
    'commission',
    'guardians',
    'blacklist_status',
    'commission_rate',
  ],
  changeMethods: [
    'new',
    'upgrade_name_symbol',
    'upgrade_icon',
    'add_to_blacklist',
    'remove_from_blacklist',
    'propose_new_owner',
    'accept_ownership',
    'extend_guardians',
    'remove_guardians',
    'destroy_black_funds',
    'pause',
    'resume',
    'ft_transfer',
    'ft_transfer_call',
    'transfer_stable_liquidity',
    'withdraw',
    'withdraw_stable_pool',
    'set_commission_rate',
  ],
};

const usdtMethods = {
  viewMethods: ['ft_balance_of'],
  changeMethods: ['new', 'mint', 'ft_transfer', 'ft_transfer_call'],
};

const refMethods = {
  viewMethods: ['get_stable_pool', 'get_deposit'],
  changeMethods: [
    'new',
    'storage_deposit',
    'add_stable_swap_pool',
    'extend_whitelisted_tokens',
  ],
};

async function sandboxSetup() {
  portUsed.check(config.port, config.domain).then(
    (inUse) => {
      if (!inUse) {
        throw new Error('Run sandbox first: `npm run sandbox:test`!');
      }
    },
    (err) => {
      console.error('Error on check:', err.message);
    }
  );

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
  await masterAccount.createAccount(config.aliceId, pubKey, config.amount);
  await masterAccount.createAccount(config.carolId, pubKey, config.amount);
  keyStore.setKey(config.networkId, config.usnId, privKey);
  keyStore.setKey(config.networkId, config.usdtId, privKey);
  keyStore.setKey(config.networkId, config.refId, privKey);
  keyStore.setKey(config.networkId, config.aliceId, privKey);
  keyStore.setKey(config.networkId, config.carolId, privKey);

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
  await usdtContract.mint({
    args: { account_id: config.aliceId, amount: '1000000000000' },
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

  await refContract.extend_whitelisted_tokens({
    args: {
      tokens: [config.usdtId, config.usnId],
    },
    amount: '1',
  });

  // Initialize other accounts connected to the contract for all test cases.
  const aliceAccount = new nearAPI.Account(near.connection, config.aliceId);
  const aliceContract = new nearAPI.Contract(
    aliceAccount,
    config.usnId,
    usnMethods
  );
  const aliceUsdt = new nearAPI.Contract(
    aliceAccount,
    config.usdtId,
    usdtMethods
  );

  const carolAccount = new nearAPI.Account(near.connection, config.carolId);
  const carolUsdt = new nearAPI.Contract(
    carolAccount,
    config.usdtId,
    usdtMethods
  );
  const carolContract = new nearAPI.Contract(
    carolAccount,
    config.usnId,
    usnMethods
  );

  // Setup a global test context.
  global.usnAccount = usnAccount;
  global.usnContract = usnContract;
  global.usdtContract = usdtContract;
  global.refContract = refContract;
  global.aliceAccount = aliceAccount;
  global.aliceUsdt = aliceUsdt;
  global.aliceContract = aliceContract;
  global.carolContract = carolContract;
  global.carolUsdt = carolUsdt;
  global.usnRef = usnRef;
}

async function sandboxTeardown() {
  const near = global.near;

  const alice = new nearAPI.Account(near.connection, config.aliceId);
  const usn = new nearAPI.Account(near.connection, config.usnId);

  await alice.deleteAccount(config.masterId);
  await usn.deleteAccount(config.masterId);
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
