// The main idea is to test the performance of indexer.
import {ApiPromise} from "@polkadot/api";
import {dolphinFullNode, indexerAddress} from "./config.json";
import createPromiseApi from "./utils";
import {assert} from "chai";
import {performance} from "perf_hooks";

describe("indexer stress test", function () {
    const max_concurrent = 3;
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
        const test_duration_sec = 60;

        let query_latency: number[] = [];

        const once_pull_ledger_call = async function (api: ApiPromise) {
            const gen_random = function (min: number, max: number): number {
                return Math.floor(Math.random() * (max - min) + min)
            }
            let ri = new Array<number>(receiver_shard_num).fill(0)
                .map(() => {
                    return gen_random(0, total_receivers / receiver_shard_num)
                });
            let si = gen_random(0, total_senders);
            let before = performance.now();
            const data = await (api.rpc as any).mantaPay.pull_ledger_diff(
                {
                    receiver_index: ri,
                    sender_index: si,
                },
                BigInt(max_receiver_num), BigInt(max_sender_num)
            );
            let cost = performance.now() - before;  // ms
            query_latency.push(cost);
            console.log("get a response at cost(%i ms): %i, %i", cost, data.receivers.length, data.senders.length)
        }

        let stop = false;
        setTimeout(function () {
            stop = true
        }, test_duration_sec * 1000);

        let count = 0;
        let task_queue = [];
        while (!stop) {
            for (let i = 0; i < max_concurrent; ++i) {
                task_queue.push(once_pull_ledger_call(indexer_apis[i]));
            }
            await Promise.all(task_queue);
            count += 1;
        }
        // calculate summary
        query_latency.sort(function (a, b) {
            return a - b;
        });
        let p99_latency = query_latency[Math.round(query_latency.length / 100 * 99)].toFixed(2);
        let qps = (query_latency.length / test_duration_sec).toFixed(2);
        let avg_latency = (query_latency.reduce((acc, val) => acc + val, 0) / query_latency.length).toFixed(2);
        console.log("stress test finish %d loop, qps = %d, avg = %d ms, p99 = %d ms", count, qps, avg_latency, p99_latency)
    })


    it.skip("stress test for connection", async function () {
        // create a lot of client to connect and disconnect.
        // why this case here is that we found in some case
        // the indexer connection will hang after N client connection.
        let max_client = 100;
        let retry_loop = 3;
        for (let i = 0; i < retry_loop; ++i) {
            let client_queue = [];
            for (let j = 0; j < max_client; ++j) {
                client_queue.push(createPromiseApi(indexerAddress));
            }
            let clients = await Promise.all(client_queue);
            clients.forEach((client) => client.disconnect());
        }
    })

    after(async function () {
        for (const api of indexer_apis) {
            await api.disconnect()
        }
    })
});