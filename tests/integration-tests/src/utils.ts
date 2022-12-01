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
    Utxo: {
        transparency: 'UtxoTransparency',
        public_asset: 'Asset',
        commitment: '[u8;32]',
    },
    UtxoTransparency: {
        _enum: ["Transparent", "Opaque"]
    },
    Asset: {
        id: '[u8;32]',
        value: 'u128',
    },
    FullIncomingNote: {
        address_partition: 'u8',
        incoming_note: 'IncomingNote',
        light_incoming_note: 'LightIncomingNote',
    },
    IncomingNote: {
        ephemeral_public_key: '[u8;32]',
        tag: '[u8;32]',
        ciphertext: '[[u8;32]; 3]'
    },
    LightIncomingNote: {
        ephemeral_public_key: '[u8;32]',
        ciphertext: '[[u8;32]; 3]'
    },
    PullResponse: {
        should_continue: 'bool',
        receivers: 'Vec<(Utxo, FullIncomingNote)>',
        senders: 'Vec<([u8; 32], OutgoingNote)>',
        senders_receivers_total: 'u128',
    },
    OutgoingNote: {
        ephemeral_public_key: '[u8;32]',
        ciphertext: '[[u8;32]; 3]'
    },
    DensePullResponse: {
        should_continue: 'bool',
        receiver_len: 'u64',
        receivers: 'String',
        sender_len: 'u64',
        senders: 'String',
        senders_receivers_total: 'u128',
        next_checkpoint: 'Checkpoint',
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
        },
        densely_pull_ledger_diff: {
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
            type: 'DensePullResponse'
        }
    }
}


const demo = {
    should_continue: true,
    senders_receivers_total: 100,
    receiver_len: 8,
    receivers: "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEB",
    sender_len: 8,
    senders: "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQ=="
}

export function decodeDensePullResponse(dense: any) {
    let receivers = Buffer.from(dense.receivers, "base64");
    let receiver_offset = 0;
    let receiver_chunk = [];

    for (let i = 0; i < dense.receiver_len; ++i) {
        let utxo = receivers.subarray(receiver_offset, receiver_offset + 32);
        receiver_offset += 32;
        let ephemeral_public_key = receivers.subarray(receiver_offset, receiver_offset + 32);
        receiver_offset += 32;
        let ciphertext = receivers.subarray(receiver_offset, receiver_offset + 68);
        receiver_offset += 32;
        receiver_chunk.push([utxo, {ephemeral_public_key: ephemeral_public_key, ciphertext: ciphertext}])
    }

    let senders = Buffer.from(dense.senders, "base64");
    let sender_offset = 0;
    let sender_chunk = [];
    for (let i = 0; i < dense.sender_len; ++i) {
        let void_number = senders.subarray(sender_offset, sender_offset + 32);
        receiver_offset += 32;
        sender_chunk.push(void_number);
    }

    return {
        should_continue: dense.should_continue,
        senders_receivers_total: dense.senders_receivers_total,
        receivers: receiver_chunk,
        senders: sender_chunk,
    }
}