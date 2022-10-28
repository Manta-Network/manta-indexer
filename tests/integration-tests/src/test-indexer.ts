import {Keyring, decodeAddress, encodeAddress} from '@polkadot/keyring';
import {hexToU8a, isHex, BN} from '@polkadot/util';
import {ApiPromise, WsProvider} from '@polkadot/api';

async function createPromiseApi(nodeAddress: string) {
    const wsProvider = new WsProvider(nodeAddress);

    const api = new ApiPromise({provider: wsProvider});
    await api.isReady;
    return api;
}

function isValidAddress(address: string) {
    try {
        encodeAddress(isHex(address) ? hexToU8a(address) : decodeAddress(address));
        return true;
    } catch (error) {
        return false;
    }
}

async function testSubNewHead(api: ApiPromise) {
    let count = 0;

    const unsubHeads = await api.rpc.chain.subscribeNewHeads((lastHeader) => {
        console.log(`last block #${lastHeader.number} has hash ${lastHeader.hash}`);

        if (++count === 10) {
            unsubHeads();
        }
    });
}

async function main() {
    let args = parse_args();
    let test_case: string = args['test_case'];

    let ws = args["ws"];
    let ws_uri;
    switch (ws) {
        case "dolphin":
            ws_uri = "wss://ws.rococo.dolphin.engineering:443";
            break;
        default:
            ws_uri = ws;
    }
    let api = await createPromiseApi(ws_uri);

    let all = (test_case === "all");
    // just test the ApiPromise connection process.
    // so when api object is created, just exit.
    if (all || test_case === "connection") {
        process.exit(0)
    }
    if (all || test_case === "sub_new_heads") {
        await testSubNewHead(api);
    }


    // const who = "dmyBqgFxMPZs1wKz8vFjv7nD4RBu4HeYhZTsGxSDU1wXQV15R";
    // const accountInfo = await api.query.system.account(who);
    // console.log(accountInfo.toHuman());
    //
    // // make a transfer
    // const seed = "//Alice";
    // const keyring = new Keyring({type: 'sr25519', ss58Format: 78});
    // const sender = keyring.addFromUri(seed);
    //
    // const amount = 12345;
    // const decimal = api.registry.chainDecimals;
    // const factor = new BN(10).pow(new BN(decimal));
    // const toTransfer = new BN(amount).mul(factor);
    // const txHash = await api.tx.balances.transfer(who, toTransfer).signAndSend(sender);
    // let counter = 0;
    // const unsub = await api.query.timestamp.now((moment: bigint) => {
    //     console.log(`the last block has a timestamp of ${moment}`)
    //     counter++;
    //     if (counter === 3) {
    //         // @ts-ignore
    //         unsub();
    //     }
    // })
}

main().catch(console.error);

function parse_args(): { [key: string]: string; } {
    return require("minimist")(process.argv.slice(2))
}