const polkaAPI = require("@polkadot/api");
const { Keyring } = require('@polkadot/keyring');

const contractAPI = require("@polkadot/api-contract");
const polkaTypes = require("@polkadot/types");

const {blake2b} = require('blakejs');
const bs58 = require('bs58');
var assert = require('assert');
const sha3 = require("sha3");
const BN = require('bn.js');

require('dotenv').config();

let tokenContract;
let bridgeContract;
let api;

const keyring = new Keyring({ type: 'sr25519' });
let registry = new polkaTypes.TypeRegistry();

function sleepAsync(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

function hashSwapMessageStruct(swap_data) {
    let registry = new polkaTypes.TypeRegistry();

    const CustomType = polkaTypes.Struct.with({
        chain_id: polkaTypes.u8,
        receiver: polkaTypes.GenericAccountId,
        sender: polkaTypes.Text,
        timestamp: polkaTypes.u64,
        amount: polkaTypes.u128,
        asset: polkaTypes.GenericAccountId,
        transfer_nonce: polkaTypes.u128
    });

    const tmp = new CustomType(registry, swap_data);

    const hash = new sha3.SHA3(256);
    hash.update(Buffer.from(tmp.toU8a()));
    return hash.digest().toJSON().data;
}

describe('Bridge', function() {
    before(async function() {
        const wsProvider = new polkaAPI.WsProvider(process.env.NODE_URL);
        api = await polkaAPI.ApiPromise.create({ provider: wsProvider });
        const tokenAbi = require('../erc20token/target/metadata.json');
        const bridgeAbi = require('../target/metadata.json');
        tokenContract = new contractAPI.ContractPromise(api, tokenAbi, process.env.TOKEN_ADDRESS);
        bridgeContract = new contractAPI.ContractPromise(api, bridgeAbi, process.env.BRIDGE_ADDRESS);
    });
    describe('seters()', function() {
        it('set fee', async function() {
            this.timeout(90000);
            let tx = await bridgeContract.tx.setFee(0, -1, 99);
            let _ = await tx.signAndSend(keyring.addFromUri('//Alice'));

            await sleepAsync(6000);

            const { gasConsumed, result, outcome } = await bridgeContract.query.getFee(keyring.addFromUri('//Alice').address, 0, -1);
            if (result.isOk) {
                assert.strictEqual(result.toHuman().Ok.data, '0x63000000000000000000000000000000');
            }

            tx = await bridgeContract.tx.setFee(0, -1, 2);
            _ = await tx.signAndSend(keyring.addFromUri('//Alice'));
        });
        it('set validators', async function() {
            this.timeout(50000);
            await sleepAsync(2000);
            let validators = [keyring.addFromUri('//Alice').address, keyring.addFromUri('//Bob').address, keyring.addFromUri('//Charlie').address, keyring.addFromUri('//Dave').address, keyring.addFromUri('//Eve').address];
            for (let i = 0; i < validators.length; i++) {
                await sleepAsync(3000);
                let tx = await bridgeContract.tx.addValidator(0, -1, validators[i]);
                let _ = await tx.signAndSend(keyring.addFromUri('//Alice'));
                await sleepAsync(3000);
            }
            await sleepAsync(2000);
            const { gasConsumed, result, outcome } = await bridgeContract.query.getValidators(keyring.addFromUri('//Alice').address, 0, -1);
    
            assert.strictEqual(result.isOk, true);
            const type = polkaTypes.createTypeUnsafe(registry, 'Vec<AccountId>', [result.toHuman().Ok.data]);
            assert.strictEqual(type.length, parseInt(process.env.CONTRACT_MAX_VALIDATORS, 10));
        });
    });
    describe('swap coin request', function() {
        before(async function() {
            this.timeout(50000);
            await sleepAsync(2000);
            let validators = [keyring.addFromUri('//Alice').address, keyring.addFromUri('//Bob').address, keyring.addFromUri('//Charlie').address, keyring.addFromUri('//Dave').address];
            for (let i = 0; i < validators.length; i++) {
                await sleepAsync(3000);
                let tx = await bridgeContract.tx.addValidator(0, -1, validators[i]);
                let _ = await tx.signAndSend(keyring.addFromUri('//Alice'));
                await sleepAsync(3000);
            }
        });
        it('1_one swap', async function() {
            this.timeout(50000);
            let swapMessage = {
                chain_id: 0,
                receiver: keyring.addFromUri('//Ferdie').address,
                sender: 'fooBar',
                timestamp: 1605078851,
                amount: 1000000000000000000n,
                asset: '',
                transfer_nonce: 0
            }

            let tx = await bridgeContract.tx.requestSwap(0, -1, swapMessage);
            let _ = await tx.signAndSend(keyring.addFromUri('//Bob'));
            await sleepAsync(5000);
            let hashedMessage = hashSwapMessageStruct(swapMessage);
            const { gasConsumed, result, outcome } = await bridgeContract.query.getCountOfApprovals(keyring.addFromUri('//Bod').address, 0, -1, hashedMessage);
            assert.strictEqual(result.toHuman().Ok.data, '0x0100');
        });
        it('2_check that swap accumulates', async function() {
            this.timeout(50000);
            let swapMessage = {
                chain_id: 0,
                receiver: keyring.addFromUri('//Ferdie').address,
                sender: 'fooBar',
                timestamp: 1605078851,
                amount: 1000000000000000000n,
                asset: '',
                transfer_nonce: 0
            }

            let tx = await bridgeContract.tx.requestSwap(0, -1, swapMessage);
            let _ = await tx.signAndSend(keyring.addFromUri('//Alice'));
            await sleepAsync(5000);
            let hashedMessage = hashSwapMessageStruct(swapMessage);
            const { gasConsumed, result, outcome } = await bridgeContract.query.getCountOfApprovals(keyring.addFromUri('//Alice').address, 0, -1, hashedMessage);
            assert.strictEqual(result.toHuman().Ok.data, '0x0200');
        });
        it('3_check that coins can be transfered', async function() {
            this.timeout(50000);
            let swapMessage = {
                chain_id: 0,
                receiver: keyring.addFromUri('//Ferdie').address,
                sender: 'fooBar',
                timestamp: 1605078851,
                amount: 1000000000000000000n,
                asset: '',
                transfer_nonce: 0
            }

            let balance = await api.query.system.account(keyring.addFromUri('//Ferdie').address);
            let balanceBefore = balance.data.free;
            let tx = await bridgeContract.tx.requestSwap(0, -1, swapMessage);
            let _ = await tx.signAndSend(keyring.addFromUri('//Charlie'));
            await sleepAsync(6000);
            let hashedMessage = hashSwapMessageStruct(swapMessage);
            const { gasConsumed, result, outcome } = await bridgeContract.query.getCountOfApprovals(keyring.addFromUri('//Charlie').address, 0, -1, hashedMessage);
            assert.strictEqual(result.toHuman().Ok.data, '0x0000');  // check that swap was removed from the smart contract after execution
            balance = await api.query.system.account(keyring.addFromUri('//Ferdie').address);
            let balacneAfter = new BN(balance.data.free);
            let transferedAmount = new BN(1000000000000000000n) - (new BN(1000000000000000000n) * new BN(process.env.CONTRACT_TRANSFER_FEE) / new BN(100));
            assert.strictEqual(balacneAfter, balanceBefore + transferedAmount);
        });
        it('4_check coin transfering', async function() {
            this.timeout(50000);
            // const { gasConsumed, result, outcome } = await bridgeContract.query.testCoinTransfer(keyring.addFromUri('//Alice').address, 0, -1, '5GVy4KCvf1p4hcyk3rEvBHt3oGCcvFFzZez3NVqthkmoFEQq', 10000000);
            let tx = await bridgeContract.tx.testCoinTransfer(0, -1, keyring.addFromUri('//Ferdie').address, 10000000000);
            let _ = await tx.signAndSend(keyring.addFromUri('//Alice'));
            await sleepAsync(5000);
            const { nonce, data: balance } = await api.query.system.account(keyring.addFromUri('//Ferdie').address);
            console.log(`balance of ${balance.free} and a nonce of ${nonce}`);
        });
    });
    describe('swap token request', function() {
        it('token swap', async function() {
            this.timeout(50000);
            let validators = [keyring.addFromUri('//Alice'), keyring.addFromUri('//Bob'), keyring.addFromUri('//Charlie')];
            let swapMessage = {
                chain_id: 0,
                receiver: keyring.addFromUri('//Ferdie').address,
                sender: 'fooBar',
                timestamp: 1605172881,
                amount: 1000000000000000,
                asset: process.env.TOKEN_ADDRESS,
                transfer_nonce: 0
            }
            let tx = await bridgeContract.tx.addToken(0, -1, process.env.TOKEN_ADDRESS, 1000000000000000);
            let _ = await tx.signAndSend(keyring.addFromUri('//Alice'));
            await sleepAsync(6000);

            let tx2 = await tokenContract.tx.addBridgeAddress(0, -1, process.env.BRIDGE_ADDRESS);
            let sig1 = await tx.signAndSend(keyring.addFromUri('//Alice'));
            await sleepAsync(6000);

            let { gasConsumed, result, outcome } = await tokenContract.query.balanceOf(keyring.addFromUri('//Alice').address, 0, -1, keyring.addFromUri('//Ferdie').address);

            console.log('Balance before');
            console.log(littleEndToHex(result.toHuman().Ok.data.slice(2)));
            
            for (let i = 0; i < validators.length; i++) {
                let tx = await bridgeContract.tx.requestSwap(0, -1, swapMessage);
                let sig2 = await tx.signAndSend(validators[i]);
                await sleepAsync(6000);
            }

            await sleepAsync(6500);

            gasConsumed, result, outcome = await tokenContract.query.balanceOf(keyring.addFromUri('//Alice').address, 0, -1, keyring.addFromUri('//Ferdie').address);

            console.log('Balance after');
            console.log(result.toHuman().Ok.data);
        });
    });
});