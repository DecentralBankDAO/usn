const exec = require('child_process').exec;
const sleep = require('util').promisify(setTimeout);
const kill = require('tree-kill');

let sandbox;

const mochaGlobalSetup = async () => {
  console.log('Start sandbox...');
  sandbox = exec('npm run sandbox:test');
  await sleep(5000);
};

const mochaGlobalTeardown = async () => {
  if (sandbox.exitCode === 1) {
    console.log('Error: Sandbox server failure. Probably, it failed to start.');
  }
  console.log('Stop sandbox...');
  kill(sandbox.pid);
};

module.exports = { mochaGlobalSetup, mochaGlobalTeardown };