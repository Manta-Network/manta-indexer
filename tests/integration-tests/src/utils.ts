import {ApiPromise, WsProvider} from "@polkadot/api";
import {decodeAddress, encodeAddress} from "@polkadot/keyring";
import {hexToU8a, isHex} from "@polkadot/util";

export function isValidAddress(address: string) {
    try {
        encodeAddress(isHex(address) ? hexToU8a(address) : decodeAddress(address));
        return true;
    } catch (error) {
        return false;
    }
}

export async function createPromiseApi(nodeAddress: string) {
    const wsProvider = new WsProvider(nodeAddress);

    const api = new ApiPromise({provider: wsProvider, types: manta_pay_types, rpc: rpc_api});
    await api.isReady;
    console.log(`${nodeAddress} has been started`);
    return api;
}

export async function delay(ms: number) {
    return new Promise((resolve) => setTimeout(resolve, ms));
}

export default createPromiseApi;


export const manta_pay_types = {
    Checkpoint: {
        receiver_index: '[u64; 256]',
        sender_index: 'u64'
    },
    EncryptedNote: {
        ephemeral_public_key: '[u8; 32]',
        ciphertext: '[u8; 68]'
    },
    PullResponse: {
        should_continue: 'bool',
        receivers: 'Vec<([u8; 32], EncryptedNote)>',
        senders: 'Vec<[u8; 32]>',
        senders_receivers_total: 'u128',
    }
};

export const rpc_api = {
    mantaPay: {
        pull_ledger_diff: {
            description: 'pull from mantaPay',
            params: [
                {
                    name: 'checkpoint',
                    type: 'Checkpoint'
                },
                {
                    name: 'max_receiver',
                    type: 'u64'
                },
                {
                    name: 'max_sender',
                    type: 'u64'
                }
            ],
            type: 'PullResponse'
        }
    }
}