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

///! Sync shards from full node.
use crate::types::{EncryptedNote, PullResponse, Utxo};
use anyhow::Result;
use codec::Encode;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::WsClient;
use manta_crypto::merkle_tree::forest::Configuration;
use manta_pay::config::MerkleTreeConfiguration;
use manta_pay::signer::Checkpoint;
use sqlx::sqlite::SqlitePool;
use std::collections::HashMap;
use std::time::Duration;
use std::time::Instant;
use tokio_stream::StreamExt;
use tracing::instrument;

#[instrument]
pub async fn synchronize_shards(
    ws: &WsClient,
    next_checkpoint: &Checkpoint,
    mut max_sender_count: u64,
    mut max_receiver_count: u64,
) -> Result<PullResponse> {
    if max_receiver_count > super::MAX_RECEIVERS || max_sender_count > super::MAX_SENDERS {
        max_receiver_count = super::MAX_RECEIVERS;
        max_sender_count = super::MAX_SENDERS;
    }
    let params = rpc_params![next_checkpoint, max_sender_count, max_receiver_count];

    let pull_response = ws
        .request::<PullResponse>("mantaPay_pull_ledger_diff", params)
        .await?;

    Ok(pull_response)
}

pub async fn reconstruct_shards_from_pull_response(
    pull_response: &PullResponse,
) -> Result<HashMap<u8, Vec<(Utxo, EncryptedNote)>>> {
    let mut shards =
        HashMap::<u8, Vec<(Utxo, EncryptedNote)>>::with_capacity(pull_response.senders.len());
    let mut stream_receivers = tokio_stream::iter(pull_response.receivers.iter());
    while let Some(receiver) = stream_receivers.next().await {
        let shard_index = MerkleTreeConfiguration::tree_index(
            &receiver
                .0
                .to_vec()
                .try_into()
                .map_err(|_| crate::IndexerError::WrongMerkleTreeIndex)?,
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
    ws_client: &WsClient,
    pool: &SqlitePool,
    max_count: (u64, u64),
) -> Result<()> {
    let mut current_checkpoint = crate::db::get_latest_check_point(pool).await?;
    let (max_sender_count, max_receiver_count) = max_count;

    let mut next_checkpoint = current_checkpoint;
    loop {
        let resp = synchronize_shards(
            ws_client,
            &next_checkpoint,
            max_sender_count,
            max_receiver_count,
        )
        .await?;
        let shards = reconstruct_shards_from_pull_response(&resp).await?;

        // update shards
        let mut stream_shards = tokio_stream::iter(shards.iter());
        while let Some((shard_index, shard)) = stream_shards.next().await {
            for (next_index, sh) in shard.iter().enumerate() {
                let offset = current_checkpoint.receiver_index[*shard_index as usize] as u64;
                if !crate::db::has_shard(pool, *shard_index, next_index as u64 + offset).await {
                    let encoded_utxo = sh.encode();
                    crate::db::insert_one_shard(
                        pool,
                        *shard_index,
                        next_index as u64 + offset,
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
            resp.senders.len(),
        )
        .await;

        // should_continue == true means the synchronization is done.
        if !resp.should_continue {
            break;
        }
        current_checkpoint = next_checkpoint;
    }

    Ok(())
}

pub async fn start_sync_shards_job(
    ws_client: &WsClient,
    pool: SqlitePool,
    max_count: (u64, u64),
    frequency: u64,
) -> Result<()> {
    let mut synced_times = 0u32;
    loop {
        // todo, add error to log
        sync_shards_from_full_node(ws_client, &pool, (max_count.0, max_count.1)).await?;
        synced_times += 1;
        println!("synced shards {synced_times} times");
        // every frequency, start to sync shards from full node.
        tokio::time::sleep(Duration::from_secs(frequency)).await;
    }
}

#[cfg(test)]
pub async fn get_check_point_from_node(ws: &str) -> Result<Checkpoint> {
    let client = crate::utils::create_ws_client(ws).await?;

    let mut current_checkpoint = Checkpoint::default();
    let (max_sender_count, max_receiver_count) = (1024 * 15, 1024 * 15);

    let mut next_checkpoint = current_checkpoint;
    loop {
        let resp = synchronize_shards(
            &client,
            &next_checkpoint,
            max_sender_count,
            max_receiver_count,
        )
        .await?;
        let shards = reconstruct_shards_from_pull_response(&resp).await?;

        // update next check point
        super::pull::calculate_next_checkpoint(
            &shards,
            &current_checkpoint,
            &mut next_checkpoint,
            resp.senders.len(),
        )
        .await;
        println!(
            "next checkpoint: {:?}, sender_index: {}, senders_receivers_total: {}",
            next_checkpoint,
            resp.senders.len(),
            resp.senders_receivers_total
        );
        if !resp.should_continue {
            break;
        }
        current_checkpoint = next_checkpoint;
    }

    Ok(next_checkpoint)
}

#[instrument]
pub async fn pull_all_shards_to_db(pool: &SqlitePool, ws: &str) -> Result<()> {
    let client = crate::utils::create_ws_client(ws).await?;

    let mut current_checkpoint = Checkpoint::default();
    let (max_sender_count, max_receiver_count) = (1024 * 15, 1024 * 15);

    let mut next_checkpoint = current_checkpoint;
    let mut times = 0u32;
    let now = Instant::now();
    loop {
        let resp = synchronize_shards(
            &client,
            &next_checkpoint,
            max_sender_count,
            max_receiver_count,
        )
        .await?;
        let shards = reconstruct_shards_from_pull_response(&resp).await?;

        let mut stream_shards = tokio_stream::iter(shards.iter());
        while let Some((shard_index, shard)) = stream_shards.next().await {
            for (next_index, sh) in shard.iter().enumerate() {
                // if the shard doesn't exist in db, insert it.
                let offset = current_checkpoint.receiver_index[*shard_index as usize] as u64;
                if !crate::db::has_shard(pool, *shard_index, next_index as u64 + offset).await {
                    let encoded_utxo = sh.encode();
                    crate::db::insert_one_shard(
                        pool,
                        *shard_index,
                        next_index as u64 + offset,
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
            resp.senders.len(),
        )
        .await;
        println!(
            "next checkpoint: {:?}, sender_index: {}, senders_receivers_total: {}",
            next_checkpoint,
            resp.senders.len(),
            resp.senders_receivers_total
        );
        if !resp.should_continue {
            break;
        }
        current_checkpoint = next_checkpoint;
        times += 1;
        println!("times: {}", times);
    }
    let t = now.elapsed().as_secs();
    println!("time cost: {}", t);

    Ok(())
}

#[instrument]
pub async fn pull_ledger_diff_from_local_node(url: &str) -> Result<f32> {
    let client = crate::utils::create_ws_client(url).await?;

    let mut current_checkpoint = Checkpoint::default();
    let (max_sender_count, max_receiver_count) = (1024 * 15, 1024 * 15);

    let mut next_checkpoint = current_checkpoint;
    let mut times = 0u32;
    let mut time_cost = 0f32;
    println!("========================");
    loop {
        let now = Instant::now();
        let resp = synchronize_shards(
            &client,
            &next_checkpoint,
            max_sender_count,
            max_receiver_count,
        )
        .await?;
        let shards = reconstruct_shards_from_pull_response(&resp).await?;

        super::pull::calculate_next_checkpoint(
            &shards,
            &current_checkpoint,
            &mut next_checkpoint,
            resp.senders.len(),
        )
        .await;
        println!(
            "next checkpoint: {:?}, sender_index: {}, senders_receivers_total: {}",
            next_checkpoint,
            resp.senders.len(),
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

        let shards = reconstruct_shards_from_pull_response(&resp).await?;
        super::pull::calculate_next_checkpoint(
            &shards,
            &current_checkpoint,
            &mut next_checkpoint,
            resp.senders.len(),
        )
        .await;
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
    use crate::{db, types::*};
    use codec::Decode;
    use manta_xt::dolphin_runtime::runtime_types::pallet_manta_pay::types::TransferPost;
    use manta_xt::{dolphin_runtime, utils, MantaConfig};
    use rand::distributions::{Distribution, Uniform};
    use std::fs::File;
    use std::io::prelude::*;
    use std::io::BufReader;

    async fn mint_one_coin() {
        let config = crate::utils::read_config().unwrap();
        let node = config["indexer"]["configuration"]["full_node"]
            .as_str()
            .unwrap();
        let api = utils::create_manta_client::<MantaConfig>(node)
            .await
            .expect("Failed to create client.");

        let file =
            File::open("./tests/integration-tests/precompile-coins/precomputed_mints_v0").unwrap();
        let mut buf_reader = BufReader::new(file);
        let mut contents = Vec::new();
        buf_reader.read_to_end(&mut contents).unwrap();
        let coin_size = 349;
        let off_set = 2;
        let start = off_set;

        let seed = "//Alice";
        let signer =
            utils::create_signer_from_string::<MantaConfig, manta_xt::sr25519::Pair>(seed).unwrap();

        let mut coin = &contents[start..start + coin_size];
        let mint = dolphin_runtime::tx()
            .manta_pay()
            .to_private(TransferPost::decode(&mut coin).unwrap());
        let block_hash = api
            .tx()
            .sign_and_submit_then_watch_default(&mint, &signer)
            .await
            .unwrap()
            .wait_for_in_block()
            .await
            .unwrap()
            .block_hash();
        println!("mint extrinsic submitted: {}", block_hash);
    }

    #[tokio::test]
    #[ignore]
    async fn mint_private_coins() {
        let url = "ws://127.0.0.1:9800";
        let api = utils::create_manta_client::<MantaConfig>(url)
            .await
            .expect("Failed to create client.");

        let file =
            File::open("./tests/integration-tests/precompile-coins/precomputed_mints_v0").unwrap();
        let mut buf_reader = BufReader::new(file);
        let mut contents = Vec::new();
        buf_reader.read_to_end(&mut contents).unwrap();
        let coin_size = 349;
        let off_set = 2;

        let seed = "//Alice";
        let signer =
            utils::create_signer_from_string::<MantaConfig, manta_xt::sr25519::Pair>(seed).unwrap();

        let batch_size = 4;
        let coin_count = (contents.len() - off_set) / coin_size;

        let mut start = off_set;
        for m in 0..coin_count / batch_size {
            let mut batch_extrinsics = Vec::with_capacity(batch_size);
            for _ in 0..batch_size {
                let mut coin = &contents[start..start + coin_size];
                let mint =
                    dolphin_runtime::runtime_types::pallet_manta_pay::pallet::Call::to_private {
                        post: TransferPost::decode(&mut coin).unwrap(),
                    };
                let mint_call =
                    dolphin_runtime::runtime_types::dolphin_runtime::Call::MantaPay(mint);
                batch_extrinsics.push(mint_call);
                // point to next coin
                start += coin_size;
            }

            // send this batch
            let batched_extrinsic = dolphin_runtime::tx().utility().batch(batch_extrinsics);
            let block_hash = api
                .tx()
                .sign_and_submit_then_watch_default(&batched_extrinsic, &signer)
                .await
                .unwrap()
                // .wait_for_in_block()
                .wait_for_finalized()
                .await
                .unwrap()
                .block_hash();
            println!("{} batch mint extrinsic submitted: {}", m, block_hash);
        }
    }

    #[tokio::test]
    async fn incremental_synchronization_should_work() {
        // sync shards from node for the first time
        let pool = crate::db::create_test_db_or_first_pull(true).await;
        assert!(pool.is_ok());
        let pool = pool.unwrap();

        let config = crate::utils::read_config().unwrap();
        let node = config["indexer"]["configuration"]["full_node"]
            .as_str()
            .unwrap();
        let client = crate::utils::create_ws_client(node).await.unwrap();

        let api = utils::create_manta_client::<MantaConfig>(&node)
            .await
            .expect("Failed to create client.");

        let (max_sender_count, max_receiver_count) = (1024 * 10, 1024 * 10);
        // let frequency = 2;
        let frequency = config["indexer"]["configuration"]["frequency"]
            .as_integer()
            .unwrap() as u64;
        let duplicated_pool = pool.clone();
        let handler = tokio::spawn(async move {
            let _ = start_sync_shards_job(
                &client,
                duplicated_pool,
                (max_sender_count, max_receiver_count),
                frequency,
            )
            .await;
        });
        let last_check_point = db::get_latest_check_point(&pool).await.unwrap();
        let last_senders_receivers_total = db::get_total_senders_receivers(&pool).await.unwrap();

        // mint one coin
        mint_one_coin().await;

        // sleep 2 * frequency more seconds.
        tokio::time::sleep(Duration::from_secs(frequency * 2)).await;

        let current_check_point = db::get_latest_check_point(&pool).await.unwrap();

        let current_senders_receivers_total = db::get_total_senders_receivers(&pool).await.unwrap();
        let count_of_new_utxo = current_senders_receivers_total - last_senders_receivers_total;
        let mut count = 0;
        for (shard_index, (last, current)) in last_check_point
            .receiver_index
            .iter()
            .zip(current_check_point.receiver_index.iter())
            .enumerate()
        {
            // ensure
            assert!(current >= last);
            if current > last {
                count += current - last;
                for i in *last..*current {
                    let _shard = dolphin_runtime::storage()
                        .manta_pay()
                        .shards(shard_index as u8, i as u64);
                    let shard = api.storage().fetch(&_shard, None).await.unwrap().unwrap();
                    assert_ne!(shard.0, [0u8; UTXO_LENGTH]);
                    assert_ne!(
                        shard.1.ephemeral_public_key,
                        [0u8; EPHEMERAL_PUBLIC_KEY_LENGTH]
                    );
                    assert_ne!(shard.1.ciphertext, [0u8; CIPHER_TEXT_LENGTH]);
                }
            }
        }
        assert_eq!(count as i64, count_of_new_utxo);
        handler.abort();
    }

    #[tokio::test]
    #[ignore]
    async fn quickly_pull_shards_from_full_node() {
        let url = "ws://127.0.0.1:9800";
        let db_path = "2-has-dolphin.db";
        let pool_size = 16u32;
        let pool = db::initialize_db_pool(db_path, pool_size).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        let now = Instant::now();
        let r = pull_all_shards_to_db(&pool, url).await;
        dbg!(now.elapsed().as_secs_f32());
        assert!(r.is_ok());
    }

    #[tokio::test]
    #[ignore]
    async fn bench_pull_ledger_diff_from_sqlite() {
        let db_path = "xdolphin.db";
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
    #[ignore]
    async fn bench_pull_shards_from_local_node() {
        let mut i_sum = 0f32;
        let mut f_sum = 0f32;
        let indexer = "ws://127.0.0.1:7788";
        let full_node = "ws://127.0.0.1:9800";
        let times = 3;
        for _i in 0..times {
            i_sum += pull_ledger_diff_from_local_node(indexer).await.unwrap();
        }
        println!("indexer time cost: {} second", i_sum / times as f32);

        for _i in 0..times {
            f_sum += pull_ledger_diff_from_local_node(full_node).await.unwrap();
        }
        println!("full node time cost: {} second", f_sum / times as f32);
    }

    #[tokio::test]
    async fn reconstruct_shards_should_be_correct() {
        // let url = "wss://ws.rococo.dolphin.engineering:443";
        let config = crate::utils::read_config().unwrap();
        let node = config["indexer"]["configuration"]["full_node"]
            .as_str()
            .unwrap();
        let client = crate::utils::create_ws_client(node).await.unwrap();

        let mut checkpoint = Checkpoint::default();
        checkpoint.sender_index = 0usize;
        let (max_sender_count, max_receiver_count) = (1024 * 16, 1024 * 16);

        let resp =
            synchronize_shards(&client, &checkpoint, max_sender_count, max_receiver_count).await;
        let resp = resp.unwrap();
        let shards = reconstruct_shards_from_pull_response(&resp).await.unwrap();

        let api = utils::create_manta_client::<MantaConfig>(&node)
            .await
            .expect("Failed to create client.");

        // check shards randomly for 5 times
        for _ in 0..5 {
            let shard_index_between = Uniform::from(0..=20); // [0, 256)
            let next_index_between = Uniform::from(0..100);
            let mut rng = rand::thread_rng();
            let shard_index = shard_index_between.sample(&mut rng);
            let next_index = next_index_between.sample(&mut rng);
            let _shard = dolphin_runtime::storage()
                .manta_pay()
                .shards(shard_index as u8, next_index as u64);
            let onchain_utxo = api.storage().fetch(&_shard, None).await.unwrap().unwrap();
            let reconstructed_utxo =
                &shards.get(&shard_index).as_ref().unwrap()[next_index as usize];

            assert_eq!(onchain_utxo.0, reconstructed_utxo.0);
            assert_eq!(
                onchain_utxo.1.ephemeral_public_key,
                reconstructed_utxo.1.ephemeral_public_key
            );
            assert_eq!(onchain_utxo.1.ciphertext, reconstructed_utxo.1.ciphertext);
        }
    }
}
