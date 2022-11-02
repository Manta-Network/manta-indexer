import { createPromiseApi } from "./utils";
import { dolphinFullNode } from "./config.json";
import { assert } from "chai";

describe("Ensure parachain node produces blocks.", function () {
  it("Check block production for parachain.", async function () {
    const dolphinApi = await createPromiseApi(dolphinFullNode);

    const currentBlockHeader = await dolphinApi.rpc.chain.getHeader();
    const blockNumber = currentBlockHeader.number.toNumber();

    assert.isTrue(blockNumber >= 1);
    await dolphinApi.disconnect();
  });
});
