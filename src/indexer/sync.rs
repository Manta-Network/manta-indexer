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

use crate::constants::PULL_LEDGER_DIFF_METHODS;
///! Sync shards from full node.
use crate::types::{EncryptedNote, PullResponse, Utxo};
use anyhow::Result;
use codec::Encode;
use frame_support::log::{error, info};
use jsonrpsee::core::client::ClientT;
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::WsClient;
use manta_crypto::merkle_tree::forest::Configuration;
use manta_pay::config::MerkleTreeConfiguration;
use manta_pay::signer::Checkpoint;
use sqlx::sqlite::SqlitePool;
use std::collections::HashMap;
use std::time::Instant;
use tokio_stream::StreamExt;
use tracing::instrument;

/// sync ledger diff from remote full node using the checkpoint as "beginning point".
#[instrument]
pub async fn synchronize_ledger(
    ws: &WsClient,
    checkpoint: &Checkpoint,
    mut max_sender_count: u64,
    mut max_receiver_count: u64,
) -> Result<PullResponse> {
    max_receiver_count = max_receiver_count.min(super::MAX_RECEIVERS);
    max_sender_count = max_sender_count.min(super::MAX_SENDERS);
    Ok(ws
        .request::<PullResponse>(
            PULL_LEDGER_DIFF_METHODS,
            rpc_params![checkpoint, max_sender_count, max_receiver_count],
        )
        .await?)
}

pub fn reconstruct_shards_from_pull_response(
    pull_response: &PullResponse,
) -> Result<HashMap<u8, Vec<(Utxo, EncryptedNote)>>> {
    let mut shards =
        HashMap::<u8, Vec<(Utxo, EncryptedNote)>>::with_capacity(pull_response.senders.len());
    for receiver in pull_response.receivers.iter() {
        let shard_index = MerkleTreeConfiguration::tree_index(
            &receiver
                .0
                .to_vec()
                .try_into()
                .map_err(|_| crate::errors::IndexerError::WrongMerkleTreeIndex)?,
        );
        shards
            .entry(shard_index)
            .or_default()
            .push(receiver.clone()); // do not use clone in the future
    }
    Ok(shards)
}

/*
1. When the indexer starts, get latest checkpoint from db.
2. With latest checkpoint, trigger one synchronization in each 2 seconds.
3. In current synchronization, if `should_continue` == true, accumlate shards to current checkpoint,
   then send next shard request until `should_continue` == false.
4. Save every new shards to each corresponding shard index, and void number.
5.
*/
#[instrument]
pub async fn sync_shards_from_full_node(
    ws: &str,
    pool: &SqlitePool,
    max_count: (u64, u64),
) -> Result<()> {
    let client = crate::utils::create_ws_client(ws).await?;

    let mut current_checkpoint = crate::db::get_latest_check_point(pool).await?;
    let (max_sender_count, max_receiver_count) = max_count;

    let mut next_checkpoint = current_checkpoint;
    loop {
        let resp = synchronize_ledger(
            &client,
            &next_checkpoint,
            max_sender_count,
            max_receiver_count,
        )
        .await?;
        let shards = reconstruct_shards_from_pull_response(&resp)?;

        // update shards
        let mut stream_shards = tokio_stream::iter(shards.iter());
        while let Some((shard_index, shard)) = stream_shards.next().await {
            for (next_index, sh) in shard.iter().enumerate() {
                // if the shard doesn't exist in db, insert it.
                if !crate::db::has_shard(pool, *shard_index, next_index as u64).await {
                    let encoded_utxo = sh.encode();
                    crate::db::insert_one_shard(
                        pool,
                        *shard_index,
                        next_index as u64,
                        encoded_utxo,
                    )
                    .await?;
                }
            }
        }

        // update void number
        let mut stream_vns = tokio_stream::iter(resp.senders.iter().enumerate());
        let vn_checkpoint = crate::db::get_len_of_void_number(pool).await?;
        while let Some((idx, vn)) = stream_vns.next().await {
            let vn: Vec<u8> = (*vn).into();
            let i = idx + vn_checkpoint;
            crate::db::insert_one_void_number(pool, i as u64, vn).await?;
        }

        // update total senders and receivers
        let new_total = resp.senders_receivers_total as i64;
        crate::db::update_or_insert_total_senders_receivers(pool, new_total).await?;

        // update next check point
        super::pull::calculate_next_checkpoint(
            &shards,
            &current_checkpoint,
            &mut next_checkpoint,
            resp.receivers.len(),
        );

        // should_continue == true means the synchronization is done.
        if !resp.should_continue {
            break;
        }
        current_checkpoint = next_checkpoint;
    }

    Ok(())
}

#[instrument]
pub async fn pull_all_shards_to_db(pool: &SqlitePool, ws: &str) -> Result<()> {
    let client = crate::utils::create_ws_client(ws).await?;

    let mut current_checkpoint = Checkpoint::default();
    let (max_sender_count, max_receiver_count) = (1024 * 8, 1024 * 8);

    let mut counter = 0;
    let mut total_items = 0;
    let mut now = Instant::now();
    loop {
        let resp = synchronize_ledger(
            &client,
            &current_checkpoint,
            max_sender_count,
            max_receiver_count,
        )
        .await?;
        if resp.senders.len() != resp.receivers.len() {
            error!(
                target: "indexer",
                "pull ledger diff sender len({}) != receiver len({}), ckpt = {:?}",
                resp.senders.len(),
                resp.receivers.len(),
                current_checkpoint
            )
        }

        let shards = reconstruct_shards_from_pull_response(&resp)?;

        // update receiver shards into sqlite.
        let mut stream_shards = tokio_stream::iter(shards.iter());
        while let Some((shard_index, shard)) = stream_shards.next().await {
            for (offset, content) in shard.iter().enumerate() {
                let encoded_utxo: Vec<u8> = content.encode();
                let next_index_beginning_offset =
                    current_checkpoint.receiver_index[*shard_index as usize];
                crate::db::insert_one_shard(
                    pool,
                    *shard_index,
                    (next_index_beginning_offset + offset) as u64,
                    encoded_utxo,
                )
                .await?;
            }
        }

        // update sender void number into sqlite.
        let mut stream_vns = tokio_stream::iter(resp.senders.iter().enumerate());
        while let Some((_, vn)) = stream_vns.next().await {
            let vn: Vec<u8> = (*vn).into();
            crate::db::append_void_number(pool, vn).await?;
        }

        // update total senders and receivers
        let new_total = resp.senders_receivers_total as i64;
        crate::db::update_or_insert_total_senders_receivers(pool, new_total).await?;

        // update next check point
        increasing_checkpoint(&mut current_checkpoint, resp.senders.len(), &shards);
        total_items += resp.receivers.len() + resp.senders.len();
        let time = now.elapsed().as_millis();
        now = Instant::now();
        info!(
            target: "indexer",
            "sync new loop({}): fetch amount: {}, total amount: {}, total amount in server: {}, cost {} ms",
            counter,
            resp.receivers.len(),
            total_items,
            resp.senders_receivers_total,
            time
        );
        if !resp.should_continue {
            break;
        }
        counter += 1;
    }
    Ok(())
}

/// According to the incremental `pull_ledger_diff` receiver and sender value,
/// move checkpoint cursor to next beginning offset.
fn increasing_checkpoint(
    checkpoint: &mut Checkpoint,
    incremental_sender: usize,
    incremental_receiver: &HashMap<u8, Vec<(Utxo, EncryptedNote)>>,
) {
    checkpoint.sender_index += incremental_sender;
    incremental_receiver.iter().for_each(|(idx, utxos)| {
        checkpoint.receiver_index[*idx as usize] += utxos.len();
    });
}

#[instrument]
pub async fn pull_ledger_diff_from_local_node() -> Result<f32> {
    let url = "ws://127.0.0.1:7788";
    let client = crate::utils::create_ws_client(url).await?;

    let mut current_checkpoint = Checkpoint::default();
    let (max_sender_count, max_receiver_count) = (1024 * 15, 1024 * 15);

    let mut next_checkpoint = current_checkpoint;
    let mut times = 0u32;
    let mut time_cost = 0f32;
    println!("========================");
    loop {
        let now = Instant::now();
        let resp = synchronize_ledger(
            &client,
            &next_checkpoint,
            max_sender_count,
            max_receiver_count,
        )
        .await?;
        let shards = reconstruct_shards_from_pull_response(&resp)?;

        super::pull::calculate_next_checkpoint(
            &shards,
            &current_checkpoint,
            &mut next_checkpoint,
            resp.receivers.len(),
        );
        println!(
            "next checkpoint: {:?}, sender_index: {}, senders_receivers_total: {}",
            next_checkpoint,
            resp.receivers.len(),
            resp.senders_receivers_total
        );
        if !resp.should_continue {
            break;
        }
        current_checkpoint = next_checkpoint;
        let t = now.elapsed().as_secs_f32();
        time_cost += t;
        times += 1;
        println!("pulling times: {}", times);
    }
    Ok(time_cost)
}

#[instrument]
pub async fn pull_ledger_diff_from_sqlite(pool: &SqlitePool) -> Result<f32> {
    let mut current_checkpoint = Checkpoint::default();
    // current_checkpoint.sender_index = 0usize;
    let (max_sender_count, max_receiver_count) = (1024 * 15, 1024 * 15);

    let mut next_checkpoint = current_checkpoint;
    let mut times = 0u32;
    let mut time_cost = 0f32;
    println!("========================");
    loop {
        let now = Instant::now();
        let resp = super::pull::pull_ledger_diff(
            pool,
            &next_checkpoint,
            max_sender_count,
            max_receiver_count,
        )
        .await?;

        let shards = reconstruct_shards_from_pull_response(&resp)?;
        super::pull::calculate_next_checkpoint(
            &shards,
            &current_checkpoint,
            &mut next_checkpoint,
            resp.receivers.len(),
        );
        if !resp.should_continue {
            break;
        }
        current_checkpoint = next_checkpoint;
        let t = now.elapsed().as_secs_f32();
        time_cost += t;
        times += 1;
        println!("pulling times: {}. cost: {}", times, t);
    }
    Ok(time_cost)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[tokio::test]
    #[ignore = "todo, use Georgi's stress test to generate related utxos."]
    async fn bench_pull_ledger_diff_from_sqlite() {
        let db_path = "latest-dolphin-shards.db";
        let pool_size = 16u32;
        let pool = db::initialize_db_pool(db_path, pool_size).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        let mut sum = 0f32;
        let times = 5;
        for _i in 0..times {
            sum += pull_ledger_diff_from_sqlite(&pool).await.unwrap();
        }
        println!("time cost: {} second", sum / times as f32);
    }

    #[tokio::test]
    #[ignore = "todo, use Georgi's stress test to generate related utxos."]
    async fn bench_pull_shards_from_local_node() {
        let mut sum = 0f32;
        let times = 1;
        for _i in 0..times {
            sum += pull_ledger_diff_from_local_node().await.unwrap();
        }
        println!("time cost: {} second", sum / times as f32);
    }

    #[tokio::test]
    #[ignore = "todo, use Georgi's stress test to generate related utxos."]
    async fn pull_ledger_diff_should_work() {
        let url = "wss://ws.rococo.dolphin.engineering:443";
        let client = crate::utils::create_ws_client(url).await.unwrap();

        let mut checkpoint = Checkpoint::default();
        checkpoint.sender_index = 0usize;
        let (max_sender_count, max_receiver_count) = (1024 * 10, 1024 * 10);

        let resp =
            synchronize_ledger(&client, &checkpoint, max_sender_count, max_receiver_count).await;
        assert!(resp.unwrap().should_continue);
    }

    #[tokio::test]
    async fn reconstruct_shards_should_be_correct() {
        let url = "wss://ws.rococo.dolphin.engineering:443";
        let client = crate::utils::create_ws_client(url).await.unwrap();

        let mut checkpoint = Checkpoint::default();
        checkpoint.sender_index = 0usize;
        let (max_sender_count, max_receiver_count) = (1024, 1024);

        let resp =
            synchronize_ledger(&client, &checkpoint, max_sender_count, max_receiver_count).await;
        let resp = resp.unwrap();
        assert!(resp.should_continue);
        let shards = reconstruct_shards_from_pull_response(&resp).unwrap();

        for (k, shard) in shards.iter() {
            for sh in shard.iter() {
                println!(
                    "k: {}, utxo: {:?}, ephemeral_public_key: {:?}, ciphertext: {:?}",
                    k,
                    hex::encode(sh.0),
                    hex::encode(sh.1.ephemeral_public_key),
                    hex::encode(sh.1.ciphertext)
                );
            }
        }

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
        let hash = crate::utils::get_storage_hash(&client).await;
        assert!(hash.is_ok());
    }
}
