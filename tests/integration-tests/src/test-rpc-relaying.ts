import { expect, assert, util } from 'chai';
import { Keyring, decodeAddress, encodeAddress } from '@polkadot/keyring';
import { hexToU8a, isHex, BN } from '@polkadot/util';
import {ApiPromise, WsProvider} from '@polkadot/api';
import { createPromiseApi, TestApis } from './utils';
import { dolphinFullNode, indexerAddress } from './config.json';

// relaying non subscription rpc should work
describe('Relaying non subscription rpc methods', async function() {
    // ensure full node and indexer service is working.
    let fullNodeApi: ApiPromise;
    let indexerApi: ApiPromise;

    before(async function() {
        // ensure full node and indexer are health
        fullNodeApi = await createPromiseApi(dolphinFullNode);
        indexerApi = await createPromiseApi(indexerAddress);
        const fullNodeHealth = await fullNodeApi.rpc.system.health();
        const indexerHealth = await indexerApi.rpc.system.health();
        assert.isNotTrue(fullNodeHealth.toJSON().isSyncing);
        assert.isTrue(indexerHealth.toJSON().isSyncing);
    });

    // relaying non subscription rpc should work

    // system_health
    it('Ensure full node and indexer service are working.', async function(done) {
        const fullNodeHealth = await fullNodeApi.rpc.system.health();
        const indexerHealth = await indexerApi.rpc.system.health();
        assert.isNotTrue(fullNodeHealth.toJSON().isSyncing);
        assert.isNotTrue(indexerHealth.toJSON().isSyncing);
        done();
    });

    // chain_getBlockHash
    it('should return a block hash', async function(done) {
        const block = 10;
        const nodeBlockHash = await fullNodeApi.rpc.chain.getBlockHash(block);
        const indexerBlockHash = await indexerApi.rpc.chain.getBlockHash(block);
        assert.equal(nodeBlockHash.toHuman(), indexerBlockHash.toHuman());
        done();
    })

    // chain_getHeader
    it('should return a block header', async function(done) {
        const blockHash = await fullNodeApi.rpc.chain.getBlockHash();

        const nodeBlockHeader = await fullNodeApi.rpc.chain.getHeader(blockHash);
        const indexerBlockHeader = await indexerApi.rpc.chain.getHeader(blockHash);
        assert.equal(nodeBlockHeader.toHuman(), indexerBlockHeader.toHuman());
        done();
    });

    // chain_getFinalizedHead
    it('should return a finalized block', async function(done) {
        const nodeFinalizedBlock = await fullNodeApi.rpc.chain.getFinalizedHead();
        const indexerFinalizedBlock = await indexerApi.rpc.chain.getFinalizedHead();
        assert.equal(nodeFinalizedBlock.toHuman(), indexerFinalizedBlock.toHuman());
        done();
    });

    // author_submitExtrinsic
    it('Submitting signed transaction payload should work', async function(done) {
        // const storageChanges = await api.rpc.author.submitExtrinsic();
        done();
    });

    // rpc_methods
    it('should return a rpc methods list', async function() {
        const rpcMethods = await indexerApi.rpc.rpc.methods();
        console.log(rpcMethods.toHuman());
    });

    // state_getMetadata
    it('Returning metdata should work', async function() {
        const metdata = await indexerApi.rpc.state.getMetadata();
        console.log(metdata.toHuman());
    });

    // state_getRuntimeVersion
    it('Returning runtime version should work', async function() {
        const runtimetVersion = await indexerApi.rpc.state.getRuntimeVersion();
        console.log(runtimetVersion.toHuman());
    });

    // system_chain
    it('Returning chain name should work', async function() {
        const chainName = await indexerApi.rpc.system.chain();
        console.log(chainName.toHuman());
    });

    // system_properties
    it('Returning chain properties should work', async function() {
        const chainProperties = await indexerApi.rpc.system.properties();
        console.log(chainProperties.toHuman());
    });

    // relaying subscription rpc should work

    // state_subscribeRuntimeVersion
    it('Retrieving the runtime version via subscription should work', async function() {
        const runtimetVersion = await indexerApi.rpc.state.subscribeRuntimeVersion();
        console.log(runtimetVersion.toHuman());
    });

    // state_subscribeStorage
    it('Subscribing storage changes for the provided keys should work', async function() {
        const storageChanges = await indexerApi.rpc.state.subscribeStorage();
    });

    // Making a normal transaction should work
    it('Subscribing storage changes for the provided keys should work', async function() {
        
        // make a transfer
        const keyring = new Keyring({ type: 'sr25519', ss58Format: 78 });
        const aliceSeed = "//Alice";
        const alice = keyring.addFromUri(aliceSeed);

        const bobSeed = "//Bob";
        const bob = keyring.addFromUri(bobSeed);
        
        // transfer 10 tokens from alice to bob
        const amount = 10;
        const decimal = indexerApi.registry.chainDecimals;
        const factor = new BN(10).pow(new BN(decimal));
        const toTransfer = new BN(amount).mul(factor);
        const aliceToBob = await indexerApi.tx.balances.transfer(bob.address, toTransfer).signAndSend(alice);
        console.log(aliceToBob.toHuman());

        // transfer 10 tokens back to ensure we will reuse both accounts for next testing.
        const bobToAlice = await indexerApi.tx.balances.transfer(alice.address, toTransfer).signAndSend(bob);
        console.log(bobToAlice.toHuman());
    });

    after(function() {
        // Exit the mocha process.
        // If not, the process will pend there.
        process.exit(0);
    });
})
