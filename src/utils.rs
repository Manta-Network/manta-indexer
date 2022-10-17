// Copyright 2020-2022 Manta Network.
// This file is part of Manta.
//
// Manta is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Manta is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Manta.  If not, see <http://www.gnu.org/licenses/>.

use crate::constants::*;
use anyhow::Result;
use codec::Encode;
use frame_support::{storage::storage_prefix, StorageHasher, Twox64Concat};
use jsonrpsee::core::client::ClientT;
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use sp_core::storage::{StorageData, StorageKey};
use std::{
    fs::File,
    io::{prelude::*, BufReader},
};
use tokio::time::{sleep, Duration};
use toml::Value;

// read project config file
pub fn read_config() -> Result<Value> {
    let config = File::open(concat!(env!("CARGO_MANIFEST_DIR"), "/config.toml"))?;
    let mut buff = BufReader::new(config);
    let mut contents = String::new();
    buff.read_to_string(&mut contents)?;

    let value = contents.parse::<Value>()?;
    Ok(value)
}

pub fn init_logger() {
    let _ = tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
}

pub fn create_full_map_key(storage_name: &[u8], key: impl Encode) -> StorageKey {
    let prefix = storage_prefix(&MANTA_PAY_KEY_PREFIX, storage_name);
    let key = Twox64Concat::hash(&key.encode());

    let full_key = prefix.into_iter().chain(key).collect();
    StorageKey(full_key)
}

pub fn create_full_doublemap_key(
    storage_name: &[u8],
    key1: impl Encode,
    key2: impl Encode,
) -> StorageKey {
    let prefix = storage_prefix(&MANTA_PAY_KEY_PREFIX, storage_name);
    let key1 = Twox64Concat::hash(&key1.encode());
    let key2 = Twox64Concat::hash(&key2.encode());
    let key = key1.into_iter().chain(key2.into_iter());

    let full_key = prefix.into_iter().chain(key).collect();
    StorageKey(full_key)
}

pub async fn create_ws_client(url: &str) -> Result<WsClient> {
    let client = WsClientBuilder::default().build(&url).await?;

    Ok(client)
}

pub async fn is_full_node(ws: &WsClient) -> bool {
    let roles = ws
        .request::<Vec<String>>("system_nodeRoles", None)
        .await
        .expect("Unknown roles.");
    roles.iter().any(|r| r == "Full")
}

pub async fn is_the_rpc_methods_existed(ws: &WsClient, rpc_method: &str) -> Result<bool> {
    use serde::{Deserialize, Serialize};
    #[derive(Clone, Debug, Deserialize, Serialize)]
    pub struct RpcMethods {
        version: u32,
        methods: Vec<String>,
    }

    println!("is connected: {}", ws.is_connected());
    let methods = ws.request::<RpcMethods>("rpc_methods", None).await?;

    Ok(methods.methods.iter().any(|m| m == rpc_method))
}

pub fn build_tokio_runtime() -> Result<tokio::runtime::Runtime> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .on_thread_start(|| {
            println!("manta indexer servier is started!");
        })
        .on_thread_stop(|| {
            println!("manta indexer servier is stoped!");
        })
        .enable_all()
        .build()?;

    Ok(rt)
}

// Ensure full node is started first.
pub async fn is_full_node_started(ws: &WsClient) -> Result<bool> {
    let current_block = ws.request::<String>("chain_getBlockHash", None).await?;

    // sleep 12-13 seconds to whether any new block is produced.
    sleep(Duration::from_secs(13)).await;
    let latest_block = ws.request::<String>("chain_getBlockHash", None).await?;

    Ok(latest_block != current_block)
}

pub async fn submit_extrinsic(ws: &WsClient, extrinsic: &str) -> Result<String> {
    let finalized_head = ws
        .request::<String>("author_submitExtrinsic", rpc_params![extrinsic])
        .await?;

    Ok(finalized_head)
}

pub async fn get_storage_hash(ws: &WsClient) -> Result<Option<String>> {
    let finalized_head = get_latest_finalized_head(ws).await?;
    let prefix = storage_prefix(&MANTA_PAY_KEY_PREFIX, &MANTA_PAY_STORAGE_SHARDS_NAME);
    let params = rpc_params![hex::encode(prefix), finalized_head];
    let storage_hash = ws
        .request::<Option<String>>("state_getStorageHash", params.clone())
        .await?;

    Ok(storage_hash)
}

pub async fn get_storage_by_key(ws: &WsClient, key: &[u8]) -> Result<StorageData> {
    let params = rpc_params![hex::encode(key)];
    let response = ws
        .request::<StorageData>("state_getStorageAt", params)
        .await?;
    Ok(response)
}

pub async fn get_latest_finalized_head(ws: &WsClient) -> Result<String> {
    let finalized_head = ws.request::<String>("chain_getFinalisedHead", None).await?;

    Ok(finalized_head)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_storage_keys_should_work() {
        let one_key = create_full_map_key(&MANTA_PAY_STORAGE_VOID_NAME, 1);
        assert_eq!(
            hex::encode(one_key.as_ref()),
            "a66d1aecfdbd14d785a4d1d8723b4beb72bf52198301292fe79f2754a64e5ee15153cb1f00942ff401000000"
        );

        let two_key = create_full_doublemap_key(&MANTA_PAY_STORAGE_SHARDS_NAME, 1, 2);
        assert_eq!(
            hex::encode(two_key.as_ref()),
            "a66d1aecfdbd14d785a4d1d8723b4beba97ed1f827296bb679b464ff1290ddc15153cb1f00942ff4010000009eb2dcce60f37a2702000000"
        );
    }

    #[tokio::test]
    async fn get_node_role_should_work() {
        let url = "wss://falafel.calamari.systems:443"; // It's a full node
        let ws = create_ws_client(url)
            .await
            .expect("failed to create ws client.");
        assert!(is_full_node(&ws).await);
    }

    #[tokio::test]
    async fn pull_rpc_method_must_exist() {
        let config = read_config().unwrap();
        let rpc_method = config["configuration"]["rpc-method"].as_str().unwrap();
        let url = "wss://ws.rococo.dolphin.engineering:443"; // It's a full node
        let ws = create_ws_client(url)
            .await
            .expect("failed to create ws client.");
        assert!(is_the_rpc_methods_existed(&ws, rpc_method).await.unwrap());
    }

    #[tokio::test]
    async fn full_node_must_be_started_first() {
        let url = "wss://ws.rococo.dolphin.engineering:443"; // It's a full node
        let ws = create_ws_client(url)
            .await
            .expect("failed to create ws client.");
        let is_started = is_full_node_started(&ws).await;
        assert!(is_started.is_ok());
        assert!(is_started.unwrap());
    }
}
