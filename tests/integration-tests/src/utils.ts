import {ApiPromise, WsProvider} from '@polkadot/api';

export async function createPromiseApi(nodeAddress: string) {
    const wsProvider = new WsProvider(nodeAddress);

    const api = new ApiPromise({provider: wsProvider});
    await api.isReady;
    return api;
}

export interface TestApis {
    fullNodeApi: ApiPromise;
    indexerApi: ApiPromise;
}

export default createPromiseApi;
