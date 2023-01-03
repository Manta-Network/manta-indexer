import { Keyring } from "@polkadot/keyring";
import { createPromiseApi, delay } from "./utils";
import { indexerAddress } from "./config.json";
import { BN } from "@polkadot/util";

async function main() {
  const indexerApi = await createPromiseApi(indexerAddress);

  const keyring = new Keyring({ type: "sr25519", ss58Format: 78 });
  const aliceSeed = "//Alice";
  const alice = keyring.addFromUri(aliceSeed);

  const amount = 100000;
  const decimal = indexerApi.registry.chainDecimals;
  const toMint = new BN(amount).mul(new BN(decimal));

  let createAssetsCalls = [];
  let mintAssetsCalls = [];
  const symbols = ["KMA", "MANTA", "DOL"];
  const assetIds = [8, 9, 10];
  for (let i = 0; i < 3; i++) {
    const assetLocaltion = {
      V1: {
        parent: 1,
        interior: { X1: { Parachain: { parachain: 2000 + i } } },
      },
    };
    const assetMetadata = {
      metadata: {
        name: symbols[i],
        symbol: symbols[i],
        decimals: 12,
        isFrozen: false,
      },
      minBalance: 1,
      isSufficient: true,
    };
    const createAssetsCall = indexerApi.tx.assetManager.registerAsset(
      assetLocaltion,
      assetMetadata
    );
    const sudoCall = indexerApi.tx.sudo.sudo(createAssetsCall);
    createAssetsCalls.push(sudoCall);

    const _mintAssetsCall = indexerApi.tx.assetManager.mintAsset(
      assetIds[i],
      alice.address,
      toMint
    );
    const mintAssetsCall = indexerApi.tx.sudo.sudo(_mintAssetsCall);
    mintAssetsCalls.push(mintAssetsCall);
  }

  // initialize create three assets
  const unsub1 = await indexerApi.tx.utility
    .batch(createAssetsCalls)
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
        unsub1();
      }
    });

  const unsub2 = await indexerApi.tx.utility
    .batch(mintAssetsCalls)
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
        unsub2();
      }
    });
  await delay(12000);

  await indexerApi.disconnect();
}

main().catch(console.error);
