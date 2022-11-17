import { Keyring } from "@polkadot/keyring";
import { createPromiseApi, delay } from "./utils";
import { indexerAddress } from "./config.json";
import { readFile } from "fs/promises";

async function main() {
  const indexerApi = await createPromiseApi(indexerAddress);

  const keyring = new Keyring({ type: "sr25519", ss58Format: 78 });
  const aliceSeed = "//Alice";
  const alice = keyring.addFromUri(aliceSeed);

  const offSet = 2;
  const coinSize = 349; // each coin size is 349.
  const coinsCount = 12;
  // send 4 mint private transactions for each batch.
  const batchSize = 4;
  const content = await readFile("precompile-coins/precomputed_mints_v0");
  const buffer = content.subarray(
    offSet + 0 * batchSize * coinSize,
    offSet + 0 * batchSize * coinSize + coinSize * coinsCount
  );
  let start = 0;
  let end = start + coinSize;
  for (let k = 0; k < coinsCount / batchSize; ++k) {
    let mintTxs = [];
    for (let i = 0; i < batchSize; ++i) {
      const mint = indexerApi.tx.mantaPay.toPrivate(
        buffer.subarray(start)
      );
      mintTxs.push(mint);
      start = end;
      end += coinSize;
    }

    const unsub = await indexerApi.tx.utility
      .batch(mintTxs)
      .signAndSend(alice, { nonce: -1 }, (result) => {
        console.log(`Current status is ${result.status}`);
        result.events.forEach(({ phase, event: { data, method, section } }) => {
          console.log(`\t' ${phase}: ${section}.${method}:: ${data}`);
        });
        if (result.status.isInBlock) {
          console.log(
            `Transaction included at blockHash ${result.status.asInBlock}`
          );
        } else if (result.status.isFinalized) {
          console.log(
            `Transaction finalized at blockHash ${result.status.asFinalized}`
          );
          unsub();
        }
      });
    await delay(12000); // wait 12s to ensure transactions are included.
  }

  console.log(`${coinsCount} transactions have been sent.`);
  await indexerApi.disconnect();
}

main().catch(console.error);