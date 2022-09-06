"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const keyring_1 = require("@polkadot/keyring");
const util_1 = require("@polkadot/util");
const api_1 = require("@polkadot/api");
async function createPromiseApi(nodeAddress) {
    const wsProvider = new api_1.WsProvider(nodeAddress);
    const api = new api_1.ApiPromise({ provider: wsProvider });
    await api.isReady;
    return api;
}
function isValidAddress(address) {
    try {
        (0, keyring_1.encodeAddress)((0, util_1.isHex)(address) ? (0, util_1.hexToU8a)(address) : (0, keyring_1.decodeAddress)(address));
        return true;
    }
    catch (error) {
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
}
main().catch(console.error);
