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
const currentTime = (Date.now() / 1000).toFixed();

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

function littleEndToHex(littleEnd) {
    let value = '';
    for ( var i = littleEnd.length - 1; i > 0; i-=2) {
        let symbol1 = littleEnd[i];
        let symbol2 = littleEnd[i-1];
        value = value.concat(symbol2);
        value = value.concat(symbol1);
    }
    return parseInt('0x'+value);
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
            let validators = [keyring.addFromUri('//Alice').address, keyring.addFromUri('//Bob').address, keyring.addFromUri('//Charlie').address, keyring.addFromUri('//Dave').address, keyring.addFromUri('//Ferdie').address];
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
                let tx = await bridgeContract.tx.addValidator(0, -1, validators[i]);
                let _ = await tx.signAndSend(keyring.addFromUri('//Alice'));
                await sleepAsync(6000);
            }
        });
        it('1_one swap', async function() {
            this.timeout(50000);
            let transferAmount = 1000000000000000000n;
            let swapMessage = {
                chain_id: process.env.CHAIN_ID,
                receiver: keyring.addFromUri('//Ferdie').address,
                sender: 'fooBar',
                timestamp: currentTime,
                amount: transferAmount,
                asset: '',
                transfer_nonce: 19
            }

            let tx = await bridgeContract.tx.requestSwap(0, -1, swapMessage);
            let _ = await tx.signAndSend(keyring.addFromUri('//Bob'));
            await sleepAsync(6000);
            let hashedMessage = hashSwapMessageStruct(swapMessage);
            const { gasConsumed, result, outcome } = await bridgeContract.query.getCountOfApprovals(keyring.addFromUri('//Bod').address, 0, -1, hashedMessage);
            assert.strictEqual(result.toHuman().Ok.data, '0x0100');
        });
        it('2_check that swap accumulates', async function() {
            this.timeout(50000);
            let transferAmount = 1000000000000000000n;
            let swapMessage = {
                chain_id: process.env.CHAIN_ID,
                receiver: keyring.addFromUri('//Ferdie').address,
                sender: 'fooBar',
                timestamp: currentTime,
                amount: transferAmount,
                asset: '',
                transfer_nonce: 19
            }

            let tx = await bridgeContract.tx.requestSwap(0, -1, swapMessage);
            let _ = await tx.signAndSend(keyring.addFromUri('//Alice'));
            await sleepAsync(6000);
            let hashedMessage = hashSwapMessageStruct(swapMessage);
            const { gasConsumed, result, outcome } = await bridgeContract.query.getCountOfApprovals(keyring.addFromUri('//Alice').address, 0, -1, hashedMessage);
            assert.strictEqual(result.toHuman().Ok.data, '0x0200');
        });
        it('3_check that coins can be transfered', async function() {
            this.timeout(50000);
            let transferAmount = 1000000000000000000n;
            let swapMessage = {
                chain_id: process.env.CHAIN_ID,
                receiver: keyring.addFromUri('//Ferdie').address,
                sender: 'fooBar',
                timestamp: currentTime,
                amount: transferAmount,
                asset: '',
                transfer_nonce: 19
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
            let transferedAmount = new BN(transferAmount).sub((new BN(transferAmount).mul(new BN(process.env.CONTRACT_TRANSFER_FEE))).div(new BN(100)));
            assert.strictEqual(balacneAfter+0, transferedAmount.add(balanceBefore)+0);
        });
        it('4_one validator sends wrong data', async function() {
            this.timeout(50000);
            let transferAmount = 1000000000000000000n;
            let swapMessage = {
                chain_id: process.env.CHAIN_ID,
                receiver: keyring.addFromUri('//Ferdie').address,
                sender: 'fooBar',
                timestamp: currentTime,
                amount: transferAmount,
                asset: '',
                transfer_nonce: 6
            };

            let balance = await api.query.system.account(keyring.addFromUri('//Eve').address);
            let eveBalanceBefore = balance.data.free;

            let validators = [keyring.addFromUri('//Alice'), keyring.addFromUri('//Bob'), keyring.addFromUri('//Charlie')];
            for (let i = 0; i < validators.length; i++) {
                if (i == validators.length - 1) {
                    swapMessage.receiver = keyring.addFromUri('//Eve').address;
                    let tx = await bridgeContract.tx.requestSwap(0, -1, swapMessage);
                    let sig2 = await tx.signAndSend(validators[i]);
                    await sleepAsync(6000);
                } else {
                    let tx = await bridgeContract.tx.requestSwap(0, -1, swapMessage);
                    let sig2 = await tx.signAndSend(validators[i]);
                    await sleepAsync(6000);
                }
            }

            balance = await api.query.system.account(keyring.addFromUri('//Eve').address);
            let eveBalanceAfter = balance.data.free;

            assert.strictEqual(eveBalanceBefore, eveBalanceBefore);
        });
        it('5_not validator try to send approval', async function() {
            this.timeout(50000);
            let transferAmount = 1000000000000000000n;
            let swapMessage = {
                chain_id: process.env.CHAIN_ID,
                receiver: keyring.addFromUri('//Ferdie').address,
                sender: 'fooBar',
                timestamp: currentTime,
                amount: transferAmount,
                asset: '',
                transfer_nonce: 4
            };

            let validators = [keyring.addFromUri('//Alice'), keyring.addFromUri('//Bob'), keyring.addFromUri('//Eve')];  // Eve here isn't a validator
            for (let i = 0; i < validators.length; i++) {
                let tx = await bridgeContract.tx.requestSwap(0, -1, swapMessage);
                let sig2 = await tx.signAndSend(validators[i]);
                await sleepAsync(6000);
            }

            let hashedMessage = hashSwapMessageStruct(swapMessage);
            const { gasConsumed, result, outcome } = await bridgeContract.query.getCountOfApprovals(keyring.addFromUri('//Alice').address, 0, -1, hashedMessage);
            let countOfApprovals = littleEndToHex(result.toHuman().Ok.data.slice(2));

            assert.strictEqual(countOfApprovals, 2);
        });
        it('6_try to send more than set limit', async function() {
            this.timeout(50000);
            let transferAmount = 1000000000000000000n;

            let tx = await bridgeContract.tx.transferCoin(transferAmount / 2n, -1, keyring.addFromUri('//Ferdie').address);
            let sig2 = await tx.signAndSend(keyring.addFromUri('//Eve'));
            await sleepAsync(6000);

            let { gasConsumed, result, outcome } = await bridgeContract.query.getTransferNonce(keyring.addFromUri('//Eve').address, 0, -1);
            let transferNonce = littleEndToHex(result.toHuman().Ok.data.slice(2));

            tx = await bridgeContract.tx.transferCoin(transferAmount, -1, keyring.addFromUri('//Ferdie').address);
            sig2 = await tx.signAndSend(keyring.addFromUri('//Eve'));
            await sleepAsync(6000);

            result = await bridgeContract.query.getTransferNonce(keyring.addFromUri('//Eve').address, 0, -1);
            let transferNonceAfter = littleEndToHex(result.result.toHuman().Ok.data.slice(2));

            assert.strictEqual(transferNonceAfter, transferNonce);
        });
        it('7_wrong chain id sent', async function() {
            this.timeout(50000);
            let transferAmount = 1000000000000000000n;
            let swapMessage = {
                chain_id: 0,
                receiver: keyring.addFromUri('//Ferdie').address,
                sender: 'fooBar',
                timestamp: currentTime,
                amount: transferAmount,
                asset: '',
                transfer_nonce: 10
            };

            let validators = [keyring.addFromUri('//Alice'), keyring.addFromUri('//Bob'), keyring.addFromUri('//Charlie')];
            for (let i = 0; i < validators.length; i++) {
                let tx = await bridgeContract.tx.requestSwap(0, -1, swapMessage);
                let sig2 = await tx.signAndSend(validators[i]);
                await sleepAsync(6000);
            }

            let hashedMessage = hashSwapMessageStruct(swapMessage);
            const { gasConsumed, result, outcome } = await bridgeContract.query.getCountOfApprovals(keyring.addFromUri('//Alice').address, 0, -1, hashedMessage);
            let countOfApprovals = littleEndToHex(result.toHuman().Ok.data.slice(2));

            assert.strictEqual(countOfApprovals, 0);
        });
    });
    describe('swap token request', function() {
        before(async function() {
            this.timeout(50000);
            let tx = await bridgeContract.tx.addToken(0, -1, process.env.TOKEN_ADDRESS, process.env.CONTRACT_COIN_DAILY_LIMIT);
            let _ = await tx.signAndSend(keyring.addFromUri('//Alice'));
            await sleepAsync(6000);

            tx = await tokenContract.tx.addBridgeAddress(0, -1, process.env.BRIDGE_ADDRESS);
            _ = await tx.signAndSend(keyring.addFromUri('//Alice'));
            await sleepAsync(6000);
        });
        it('1_positive token swap', async function() {
            this.timeout(50000);
            let validators = [keyring.addFromUri('//Alice'), keyring.addFromUri('//Bob'), keyring.addFromUri('//Charlie')];
            let amountToTransfer = 10000
            let swapMessage = {
                chain_id: process.env.CHAIN_ID,
                receiver: keyring.addFromUri('//Ferdie').address,
                sender: 'fooBar',
                timestamp: currentTime,
                amount: amountToTransfer,
                asset: process.env.TOKEN_ADDRESS,
                transfer_nonce: 3
            }

            let result = await tokenContract.query.balanceOf(keyring.addFromUri('//Alice').address, 0, -1, keyring.addFromUri('//Ferdie').address);

            let balanceBefore = littleEndToHex(result.result.toHuman().Ok.data.slice(2));
            
            for (let i = 0; i < validators.length; i++) {
                let tx = await bridgeContract.tx.requestSwap(0, -1, swapMessage);
                let sig2 = await tx.signAndSend(validators[i]);
                await sleepAsync(6000);
            }

            await sleepAsync(6000);

            result = await tokenContract.query.balanceOf(keyring.addFromUri('//Alice').address, 0, -1, keyring.addFromUri('//Ferdie').address);

            let balanceAfter = littleEndToHex(result.result.toHuman().Ok.data.slice(2));
            let transferedAmount = amountToTransfer - (amountToTransfer * process.env.CONTRACT_TRANSFER_FEE / 100);

            assert.strictEqual(balanceAfter, balanceBefore + transferedAmount);
        });
        it('2_one validator sends wrong data', async function() {
            this.timeout(50000);
            let swapMessage = {
                chain_id: process.env.CHAIN_ID,
                receiver: keyring.addFromUri('//Ferdie').address,
                sender: 'fooBar',
                timestamp: currentTime,
                amount: 1000000000000000000n,
                asset: process.env.TOKEN_ADDRESS,
                transfer_nonce: 1
            };

            let result = await tokenContract.query.balanceOf(keyring.addFromUri('//Alice').address, 0, -1, keyring.addFromUri('//Eve').address);

            let eveBalanceBefore = littleEndToHex(result.result.toHuman().Ok.data.slice(2));

            let validators = [keyring.addFromUri('//Alice'), keyring.addFromUri('//Bob'), keyring.addFromUri('//Charlie')];
            for (let i = 0; i < validators.length; i++) {
                if (i == validators.length - 1) {
                    swapMessage.receiver = keyring.addFromUri('//Eve').address;
                    let tx = await bridgeContract.tx.requestSwap(0, -1, swapMessage);
                    let sig2 = await tx.signAndSend(validators[i]);
                    await sleepAsync(6000);
                } else {
                    let tx = await bridgeContract.tx.requestSwap(0, -1, swapMessage);
                    let sig2 = await tx.signAndSend(validators[i]);
                    await sleepAsync(6000);
                }
            }

            result = await tokenContract.query.balanceOf(keyring.addFromUri('//Alice').address, 0, -1, keyring.addFromUri('//Eve').address);

            let eveBalanceAfter = littleEndToHex(result.result.toHuman().Ok.data.slice(2));

            assert.strictEqual(eveBalanceBefore, eveBalanceAfter);
        });
        it('3_not validator try to send approval', async function() {
            this.timeout(50000);
            let amountToTransfer = 10000
            let swapMessage = {
                chain_id: process.env.CHAIN_ID,
                receiver: keyring.addFromUri('//Ferdie').address,
                sender: 'fooBar',
                timestamp: currentTime,
                amount: amountToTransfer,
                asset: process.env.TOKEN_ADDRESS,
                transfer_nonce: 77094
            };

            let hashedMessage = hashSwapMessageStruct(swapMessage);

            let validators = [keyring.addFromUri('//Alice'), keyring.addFromUri('//Bob'), keyring.addFromUri('//Eve')];  // Eve here isn't a validator
            for (let i = 0; i < validators.length; i++) {
                let tx = await bridgeContract.tx.requestSwap(0, -1, swapMessage);
                let sig2 = await tx.signAndSend(validators[i]);
                await sleepAsync(6000);
                const { gasConsumed, result, outcome } = await bridgeContract.query.getCountOfApprovals(keyring.addFromUri('//Alice').address, 0, -1, hashedMessage);
                let countOfApprovals = littleEndToHex(result.toHuman().Ok.data.slice(2));
            }

            const { gasConsumed, result, outcome } = await bridgeContract.query.getCountOfApprovals(keyring.addFromUri('//Alice').address, 0, -1, hashedMessage);
            let countOfApprovals = littleEndToHex(result.toHuman().Ok.data.slice(2));

            assert.strictEqual(countOfApprovals, 2);
        });
        it('6_try to send more than set limit', async function() {
            this.timeout(50000);
            let transferAmount = 1000000000000000000n;

            let mintTx = await tokenContract.tx.mint(0, -1, new BN(process.env.CONTRACT_COIN_DAILY_LIMIT).mul(new BN(2)), keyring.addFromUri('//Eve').address);
            let signMint = await mintTx.signAndSend(keyring.addFromUri('//Alice'));
            await sleepAsync(6000);

            let tx = await bridgeContract.tx.transferToken(0, -1, keyring.addFromUri('//Ferdie').address, new BN(process.env.CONTRACT_COIN_DAILY_LIMIT).div(new BN(2)), process.env.TOKEN_ADDRESS);
            let sig2 = await tx.signAndSend(keyring.addFromUri('//Eve'));
            await sleepAsync(6000);

            let { gasConsumed, result, outcome } = await bridgeContract.query.getTransferNonce(keyring.addFromUri('//Eve').address, 0, -1);
            let transferNonce = littleEndToHex(result.toHuman().Ok.data.slice(2));

            tx = await bridgeContract.tx.transferToken(0, -1, keyring.addFromUri('//Ferdie').address, process.env.CONTRACT_COIN_DAILY_LIMIT, process.env.TOKEN_ADDRESS);
            sig2 = await tx.signAndSend(keyring.addFromUri('//Eve'));
            await sleepAsync(6000);

            result = await bridgeContract.query.getTransferNonce(keyring.addFromUri('//Eve').address, 0, -1);
            let transferNonceAfter = littleEndToHex(result.result.toHuman().Ok.data.slice(2));

            assert.strictEqual(transferNonceAfter, transferNonce);
        });
    });
});