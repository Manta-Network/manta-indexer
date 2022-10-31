import {ApiPromise, WsProvider} from '@polkadot/api';

export async function createPromiseApi(nodeAddress: string) {
    const wsProvider = new WsProvider(nodeAddress);

    const api = new ApiPromise({provider: wsProvider});
    await api.isReady;
    console.log(`${nodeAddress} has bee started` );
    return api;
}

export default createPromiseApi;
