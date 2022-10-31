import { assert } from 'chai';
import { Keyring } from '@polkadot/keyring';
import { BN } from '@polkadot/util';
import { ApiPromise } from '@polkadot/api';
import { createPromiseApi } from './utils';
import { dolphinFullNode, indexerAddress } from './config.json';

// relaying non subscription rpc should work
describe('Relaying non subscription rpc methods', function() {
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
        assert.isNotTrue(indexerHealth.toJSON().isSyncing);
        // done();
    });

    // 1. Relaying non subscription rpc should work

    // system_health
    it('Ensure full node and indexer service are working.', async function() {
        const fullNodeHealth = await fullNodeApi.rpc.system.health();
        const indexerHealth = await indexerApi.rpc.system.health();

        // the full node is fully synced.
        assert.isNotTrue(fullNodeHealth.toJSON().isSyncing);
        assert.isNotTrue(indexerHealth.toJSON().isSyncing);
    });

    // chain_getBlockHash
    it('should return a block hash', async function() {
        const nodeBlockHash = await fullNodeApi.rpc.chain.getBlockHash();
        const indexerBlockHash = await indexerApi.rpc.chain.getBlockHash();
        assert.equal(nodeBlockHash.toHuman(), indexerBlockHash.toHuman());

        // feeding a specified block hash should work as well
        const at = await fullNodeApi.rpc.chain.getHeader();
        const _nodeBlockHash = await fullNodeApi.rpc.chain.getBlockHash(at.hash);
        const _indexerBlockHash = await indexerApi.rpc.chain.getBlockHash(at.hash);
        assert.equal(_nodeBlockHash.toHuman(), _indexerBlockHash.toHuman());
    })

    // chain_getHeader,
    it('should return a block hash', async function() {
        const nodeBlockHeader = await fullNodeApi.rpc.chain.getHeader();
        const indexerBlockHeader = await indexerApi.rpc.chain.getHeader();
        assert.equal(nodeBlockHeader.parentHash.toString(), indexerBlockHeader.parentHash.toString());
        assert.equal(nodeBlockHeader.number.toString(), indexerBlockHeader.number.toString());
        assert.equal(nodeBlockHeader.toString(), indexerBlockHeader.toString());

        // feeding a specified block hash should work as well
        const at = await fullNodeApi.rpc.chain.getHeader();
        const _nodeBlockHash = await fullNodeApi.rpc.chain.getBlockHash(at.hash);
        const _indexerBlockHash = await indexerApi.rpc.chain.getBlockHash(at.hash);
        assert.equal(_nodeBlockHash.toString(), _indexerBlockHash.toString());
    })
    
    // chain_getFinalizedHead, x
    it('should return a finalized block', async function() {
        const nodeFinalizedBlock = await fullNodeApi.rpc.chain.getFinalizedHead();
        const indexerFinalizedBlock = await indexerApi.rpc.chain.getFinalizedHead();
        assert.equal(nodeFinalizedBlock.toString(), indexerFinalizedBlock.toString());
    });
    
    // author_submitExtrinsic, x
    it.skip('Submitting signed transaction payload should work', async function() {
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

        const indexerRuntimetVersion = await indexerApi.rpc.state.getRuntimeVersion();
        const indexerBlockHash = await indexerApi.rpc.chain.getBlockHash();
        const nonce = await indexerApi.rpc.system.accountNextIndex(alice.address);

        const aliceToBobExtrinsic = indexerApi.tx.balances.transferKeepAlive(bob.address, toTransfer).sign(keyring.getPair(alice.address), {
            blockHash: indexerBlockHash,
            genesisHash: indexerApi.genesisHash,
            nonce,
            runtimeVersion: indexerApi.runtimeVersion,
        });

        const blockHash = await indexerApi.rpc.author.submitExtrinsic(aliceToBobExtrinsic);
        console.log(blockHash);
    });
    
    // rpc_methods
    it('should return a rpc methods list', async function() {
        const fullNodeRpcMethods = await fullNodeApi.rpc.rpc.methods();
        const indexerRpcMethods = await indexerApi.rpc.rpc.methods();
        assert.equal(fullNodeRpcMethods.version.toNumber(), indexerRpcMethods.version.toNumber());
        assert.equal(fullNodeRpcMethods.methods.length, indexerRpcMethods.methods.length);
        assert.equal(fullNodeRpcMethods.methods.toString(), indexerRpcMethods.methods.toString());
        for (let i = 0; i < fullNodeRpcMethods.methods.length; ++i) {
            assert.equal(fullNodeRpcMethods.methods[i].toString(), indexerRpcMethods.methods[i].toString());
        }
    });
    
    // state_getMetadata
    it('Returning metdata should work', async function() {
        const fullNodeMetdata = await fullNodeApi.rpc.state.getMetadata();
        const indexerMetdata = await indexerApi.rpc.state.getMetadata();
        assert.equal(fullNodeMetdata.magicNumber.toString(), indexerMetdata.magicNumber.toString());
        assert.equal(fullNodeMetdata.asV14.toString(), indexerMetdata.asV14.toString());

        // feeding a specified block hash should work as well, x
        const at = await fullNodeApi.rpc.chain.getHeader();
        const _fullNodeMetdata = await fullNodeApi.rpc.state.getMetadata(at.hash);
        const _indexerMetdata = await indexerApi.rpc.state.getMetadata(at.hash);
        assert.equal(_fullNodeMetdata.toString(), _indexerMetdata.toString());
    });
    
    // state_getRuntimeVersion
    it('Returning runtime version should work', async function() {
        const fullNodeRuntimetVersion = await fullNodeApi.rpc.state.getRuntimeVersion();
        const indexerRuntimetVersion = await indexerApi.rpc.state.getRuntimeVersion();
        assert.equal(fullNodeRuntimetVersion.specName.toString(), indexerRuntimetVersion.specName.toString());
        assert.equal(fullNodeRuntimetVersion.implName.toString(), indexerRuntimetVersion.implName.toString());
        assert.equal(fullNodeRuntimetVersion.authoringVersion.toNumber(), indexerRuntimetVersion.authoringVersion.toNumber());
        assert.equal(fullNodeRuntimetVersion.specVersion.toNumber(), indexerRuntimetVersion.specVersion.toNumber());
        assert.equal(fullNodeRuntimetVersion.implVersion.toNumber(), indexerRuntimetVersion.implVersion.toNumber());
        assert.equal(fullNodeRuntimetVersion.apis.toString(), indexerRuntimetVersion.apis.toString());

        // feeding a specified block hash should work as well
        const at = "";
        const nodeBlockHash = await fullNodeApi.rpc.chain.getBlockHash(at);
        const _fullNodeRuntimetVersion = await fullNodeApi.rpc.state.getRuntimeVersion(nodeBlockHash);
        const _indexerRuntimetVersion = await indexerApi.rpc.state.getRuntimeVersion(nodeBlockHash);
        assert.equal(_fullNodeRuntimetVersion.toString(), _indexerRuntimetVersion.toString());
    });
    
    // system_chain
    it('Returning chain name should work', async function() {
        const fullNodeChainName = await fullNodeApi.rpc.system.chain();
        const indexerChainName = await indexerApi.rpc.system.chain();
        assert.equal(fullNodeChainName.toString(), indexerChainName.toString());
    });
    
    // system_properties
    it('Returning chain properties should work', async function() {
        const fullNodeChainProperties = await fullNodeApi.rpc.system.properties();
        const indexerChainProperties = await indexerApi.rpc.system.properties();
        assert.equal(fullNodeChainProperties.toString(), indexerChainProperties.toString());
    });
    
    // 2. relaying subscription rpc should work
    
    // state_subscribeRuntimeVersion, x
    it('Retrieving the runtime version via subscription should work', async function() {
        const fullNodeRuntimetVersion = await fullNodeApi.rpc.state.getRuntimeVersion();

        const unsubscribe = await indexerApi.rpc.state.subscribeRuntimeVersion((version) => {
            assert.equal(fullNodeRuntimetVersion.toString(), version.toString());
        });
        
        setTimeout(() => {
            unsubscribe();
            console.log('Unsubscribed');
        }, 2000);
    });
    
    // state_subscribeStorage
    it.only('Subscribing storage changes for the provided keys should work', async function() {
        const keys = ["0xf0c365c3cf59d671eb72da0e7a4113c49f1f0515f462cdcf84e0f1d6045dfcbb"];
        const unsubscribe = await indexerApi.rpc.state.subscribeStorage(keys ,(storageChanges) => {
            console.log(`storage: #${storageChanges}`);
        });

        setTimeout(() => {
            unsubscribe();
            console.log('Unsubscribed');
        }, 2000);
    });

    // Making a normal transaction should work
    it.skip('Making a normal transaction should work', async function() {
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

    after(async function() {
        // Exit the mocha process.
        // If not, the process will pend there.
        await fullNodeApi.disconnect();
        await indexerApi.disconnect();
    });
})
