import {Keyring, decodeAddress, encodeAddress} from '@polkadot/keyring';
import {hexToU8a, isHex, BN} from '@polkadot/util';
import {ApiPromise, WsProvider} from'@polkadot/api';

async function createPromiseApi(nodeAddress: string) {
    const wsProvider = new WsProvider(nodeAddress);

    const api = new ApiPromise({ provider: wsProvider });
    await api.isReady;
    return api;
}

function isValidAddress(address: string) {
    try {
        encodeAddress(isHex(address) ? hexToU8a(address): decodeAddress(address));
        return true;  
    } catch (error) {
        return false;
    }
}

async function main() {
    let addr = "ws://127.0.0.1:9988";
    // let addr = "wss://ws.rococo.dolphin.engineering:443";
    let api = await createPromiseApi(addr);

    const who = "dmyBqgFxMPZs1wKz8vFjv7nD4RBu4HeYhZTsGxSDU1wXQV15R";
    const accountInfo = await api.query.system.account(who);
    console.log(accountInfo.toHuman());

    // make a transfer
    const seed = "//Alice";
    const keyring = new Keyring({ type: 'sr25519', ss58Format: 78 });
    const sender = keyring.addFromUri(seed);

    const amount = 12345;
    const decimal = api.registry.chainDecimals;
    const factor = new BN(10).pow(new BN(decimal));
    const toTransfer = new BN(amount).mul(factor);
    const txHash = await api.tx.balances.transfer(who, toTransfer).signAndSend(sender);
    console.log(txHash);
}

main().catch(console.error);
