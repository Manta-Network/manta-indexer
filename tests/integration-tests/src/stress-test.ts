// The main idea is to test the performance of indexer.
import {ApiPromise} from "@polkadot/api";
import {dolphinFullNode, indexerAddress} from "./config.json";
import createPromiseApi from "./utils";
import {assert} from "chai";

describe("indexer stress test", function () {
    const max_concurrent = 10;
    const receiver_shard_num = 256;
    const max_receiver_num = 1024 * 8;
    const max_sender_num = 1024 * 8;

    let indexer_apis: Array<ApiPromise>;
    let full_node_apis: Array<ApiPromise>;

    before(async function () {
        indexer_apis = [];
        // initial batch of apis as request simulation pool.
        for (let i = 0; i < max_concurrent; ++i) {
            const indexer_api = await createPromiseApi(indexerAddress);
            const health = await indexer_api.rpc.system.health();
            assert.isNotTrue(health.toJSON().isSyncing);
            indexer_apis.push(indexer_api);
        }
    });

    it("stress test", async function () {
        const total_receivers = 15000000;
        const total_senders = 8000000;

        const gen_random = function (min: number, max: number): number {
            return Math.floor(Math.random() * (max - min) + min)
        }
        for (let i = 0; i < indexer_apis.length; ++i) {
            let ri = new Array<number>(receiver_shard_num).fill(0);
            ri.forEach((_, index) => {
                ri[index] = gen_random(0, total_receivers / receiver_shard_num)
            });
            await (indexer_apis[i].rpc as any).mantaPay.pull_ledger_diff(
                {
                    receiver_index: ri,
                    sender_index: gen_random(0, total_senders),
                },
                BigInt(max_receiver_num), BigInt(max_sender_num)
            );
        }

        assert.isTrue(true);
    })

    after(async function () {
        for (const api of indexer_apis) {
            await api.disconnect()
        }
    })
});