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

  const api = new ApiPromise({ provider: wsProvider });
  await api.isReady;
  console.log(`${nodeAddress} has bee started`);
  return api;
}

export async function delay(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export default createPromiseApi;
