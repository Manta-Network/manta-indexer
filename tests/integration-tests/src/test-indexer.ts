import { Keyring } from "@polkadot/keyring";
import { BN } from "@polkadot/util";
import { createPromiseApi } from "./utils";
import { indexerAddress } from "./config.json";

async function main() {
  const indexerApi = await createPromiseApi(indexerAddress);
  // make a transfer
  const keyring = new Keyring({ type: "sr25519", ss58Format: 78 });
  const aliceSeed = "//Alice";
  const alice = keyring.addFromUri(aliceSeed);

  const bobSeed = "//Bob";
  const bob = keyring.addFromUri(bobSeed);

  // transfer 10 tokens from alice to bob
  const amount = 10;
  const decimal = indexerApi.registry.chainDecimals;
  const factor = new BN(10).pow(new BN(decimal));
  const toTransfer = new BN(amount).mul(factor);
  const unsub = await indexerApi.tx.balances
    .transfer(bob.address, toTransfer)
    .signAndSend(alice, (result) => {
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
  await indexerApi.disconnect();
}

main().catch(console.error);
