import { ApiPromise, WsProvider } from "@polkadot/api";
import { decodeAddress, encodeAddress } from "@polkadot/keyring";
import { hexToU8a, isHex } from "@polkadot/util";

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

  const api = new ApiPromise({
    provider: wsProvider,
    types: manta_pay_types,
    rpc: rpc_api,
  });
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
    receiver_index: "[u64; 256]",
    sender_index: "u64",
  },
  Utxo: {
    transparency: "UtxoTransparency",
    public_asset: "Asset",
    commitment: "[u8;32]",
  },
  UtxoTransparency: {
    _enum: ["Transparent", "Opaque"],
  },
  Asset: {
    id: "[u8;32]",
    value: "u128",
  },
  FullIncomingNote: {
    address_partition: "u8",
    incoming_note: "IncomingNote",
    light_incoming_note: "LightIncomingNote",
  },
  IncomingNote: {
    ephemeral_public_key: "[u8;32]",
    tag: "[u8;32]",
    ciphertext: "[[u8;32]; 3]",
  },
  LightIncomingNote: {
    ephemeral_public_key: "[u8;32]",
    ciphertext: "[[u8;32]; 3]",
  },
  PullResponse: {
    should_continue: "bool",
    receivers: "Vec<(Utxo, FullIncomingNote)>",
    senders: "Vec<([u8; 32], OutgoingNote)>",
    senders_receivers_total: "u128",
  },
  OutgoingNote: {
    ephemeral_public_key: "[u8;32]",
    ciphertext: "[[u8;32]; 3]",
  },
  DensePullResponse: {
    should_continue: "bool",
    receivers: "String",
    senders: "String",
    senders_receivers_total: "u128",
    next_checkpoint: "Checkpoint",
  }
};

export const rpc_api = {
  mantaPay: {
    pull_ledger_diff: {
      description: "pull from mantaPay",
      params: [
        {
          name: "checkpoint",
          type: "Checkpoint",
        },
        {
          name: "max_receiver",
          type: "u64",
        },
        {
          name: "max_sender",
          type: "u64",
        },
      ],
      type: "PullResponse",
    },
    densely_pull_ledger_diff: {
      description: "pull from mantaPay",
      params: [
        {
          name: "checkpoint",
          type: "Checkpoint",
        },
        {
          name: "max_receiver",
          type: "u64",
        },
        {
          name: "max_sender",
          type: "u64",
        },
      ],
      type: "DensePullResponse",
    },
  },
};
