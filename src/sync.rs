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
use crate::types::{EncryptedNote, PullResponse, Utxo};
use crate::utils;
use anyhow::Result;
use frame_support::storage::storage_prefix;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use manta_crypto::merkle_tree::forest::Configuration;
use manta_pay::config::MerkleTreeConfiguration;
use manta_pay::signer::Checkpoint;
use sp_core::storage::StorageData;
use std::collections::HashMap;
use tokio_stream::StreamExt;

pub async fn synchronize_shards(ws: &WsClient) -> Result<PullResponse> {
    let mut checkpoint = Checkpoint::default();
    checkpoint.sender_index = 0usize;
    let params = rpc_params![checkpoint, 1024, 1024];

    let pull_response = ws
        .request::<PullResponse>("mantaPay_pull_ledger_diff", params.clone())
        .await?;

    // todo, handle should_continue == true.

    Ok(pull_response)
}

pub fn reconstruct_shards_from_pull_response(
    pull_response: &PullResponse,
) -> Result<HashMap<u8, Vec<(Utxo, EncryptedNote)>>> {
    let mut shards =
        HashMap::<u8, Vec<(Utxo, EncryptedNote)>>::with_capacity(pull_response.senders.len());
    for receiver in pull_response.receivers.iter() {
        let shard_index =
            MerkleTreeConfiguration::tree_index(&receiver.0.to_vec().try_into().unwrap()); // remove upwrap
        shards
            .entry(shard_index)
            .or_default()
            .push(receiver.clone()); // do not use clone in the future
    }
    Ok(shards)
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

pub async fn get_storage_hash(ws: &WsClient) -> Result<Option<String>> {
    let finalized_head = get_latest_finalized_head(ws).await?;
    let prefix = storage_prefix(&MANTA_PAY_KEY_PREFIX, &MANTA_PAY_STORAGE_SHARDS_NAME);
    let params = rpc_params![hex::encode(prefix), finalized_head];
    let storage_hash = ws
        .request::<Option<String>>("state_getStorageHash", params.clone())
        .await?;

    Ok(storage_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn wss_dummy_test() {
        crate::utils::init_logger();

        let url = "wss://falafel.calamari.systems:443";
        let client = crate::utils::create_ws_client(url).await.unwrap();
        let response: String = client.request("system_chain", None).await.unwrap();
        assert_eq!(&response, "Calamari Parachain");
    }

    #[tokio::test]
    async fn pull_ledger_diff_should_work() {
        let url = "wss://ws.rococo.dolphin.engineering:443";
        let client = crate::utils::create_ws_client(url).await.unwrap();
        let resp = synchronize_shards(&client).await;
        assert!(&resp.is_ok());
        assert!(resp.unwrap().should_continue);
    }

    #[tokio::test]
    async fn reconstruct_shards_should_be_correct() {
        let url = "wss://ws.rococo.dolphin.engineering:443";
        let client = crate::utils::create_ws_client(url).await.unwrap();
        let resp = synchronize_shards(&client).await;
        assert!(&resp.is_ok());
        let resp = resp.unwrap();
        assert!(resp.should_continue);
        let shards = reconstruct_shards_from_pull_response(&resp);
        assert!(shards.is_ok());
        let shards = shards.unwrap();

        // Get first utxo, ensure it's reconstructed successfully.
        assert_eq!(
            hex::encode(&shards.get(&0u8).as_ref().unwrap()[0].0),
            "5dc4df9b2772ecf49848c7fcb6b96ce7dd2f17c43e25d67719cd7c2659db6817"
        );
        assert_eq!(
            hex::encode(&shards.get(&0u8).as_ref().unwrap()[0].1.ephemeral_public_key),
            "7a9bb8c7d0afae981506f5c50041d4252cbc7c1fa82f5be6075e429a588ff352"
        );
        assert_eq!(
            hex::encode(&shards.get(&0u8).as_ref().unwrap()[0].1.ciphertext),
            "4ba8c51afa4ef30a05bfabf7db6a69c80f89eccc8fd0e7c80176386554046b64c3293cfe053ab2f56643fdf15eb5983554a955021b28beaea998899359f95be57d9b6f95"
        );
    }

    #[tokio::test]
    async fn get_storage_hash_should_work() {
        let url = "wss://ws.rococo.dolphin.engineering:443";
        let client = crate::utils::create_ws_client(url).await.unwrap();
        let hash = get_storage_hash(&client).await;
        assert!(hash.is_ok());
    }
}
