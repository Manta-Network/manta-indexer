# manta-indexer

Indexer is doing two things now:
1. Forwarding all rpc requests from Dapp.
2. Sync `Utxo` from manta node, so Dapp can get `Utxo` more quickly.

## Indexer Design

Please take a look at [indexer design](./indexer-design.md) if you're interested.

## Perquisites
- rustc >= 1.58
- Sqlite >= 3.0
- yarn, nodejs >= 16.0

## Deployment

1. Build the binary.
```shell
cargo b --profile production
```

2. Get and configure the config files.
You should copy both folders `conf` and `migrations` to where you want to put.
Please configure the [config.toml](./conf/config.toml)
```toml
[indexer.configuration]
frequency = 2
rpc_method = "mantaPay_pull_ledger_diff"
full_node = "ws://127.0.0.1:9800"
port = 7788
prometheus_port = 7789
```

3. Run indexer.
```shell
./manta-indexer --config-path ./conf --migrations-path ./migrations
```

## How to run tests

1. Setup a local relaychain network with polkadot-launch. You can use this [json file](./.github/resources/dolphin.json) to launch those nodes.
2. Start indexer.
```
cargo r
```

3. Run some typescript based test cases.
```shell
cd tests/integration-tests
yarn
yarn initialize-assets # configure public assets
yarn mint # mint some private assets for generating Utxos
yarn test-relayer
```

4. Run some rust based test cases by cargo. This step is based on step 3 due to some utxos generated by that steps.
```
cargo test
```
