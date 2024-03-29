// Copyright 2020-2023 Manta Network.
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
use crate::constants::PULL_LEDGER_DIFF_METHODS;
use crate::indexer::cache::{put_batch_receiver, put_batch_sender};
use crate::monitoring::indexer_ledger_opts;
use crate::types::{Checkpoint, FullIncomingNote, PullResponse, Utxo};
use crate::utils::SHUTDOWN_FLAG;
use anyhow::Result;
use codec::Encode;
use frame_support::log::{debug, error, info};
use jsonrpsee::core::client::ClientT;
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::WsClient;
use manta_crypto::merkle_tree::forest::Configuration;
use manta_pay::{
    config::utxo::{MerkleTreeConfiguration, UtxoAccumulatorItemHash},
    manta_parameters::{self, Get},
    manta_util::codec::Decode as _,
};
use once_cell::sync::Lazy;
use prometheus::{register_int_counter, IntCounter};
use sqlx::sqlite::SqlitePool;
use sqlx::Acquire;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::time::Duration;
use std::time::Instant;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;

/// sync ledger diff from remote full node using the checkpoint as "beginning point".
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

pub async fn reconstruct_shards_from_pull_response(
    pull_response: &PullResponse,
) -> Result<HashMap<u8, Vec<(Utxo, FullIncomingNote)>>> {
    let mut shards =
        HashMap::<u8, Vec<(Utxo, FullIncomingNote)>>::with_capacity(pull_response.receivers.len());
    let mut stream_receivers = tokio_stream::iter(pull_response.receivers.iter());
    let utxo_accumulator_item_hash = UtxoAccumulatorItemHash::decode(
        manta_parameters::pay::parameters::UtxoAccumulatorItemHash::get()
            .expect("Checksum did not match."),
    )
    .expect("Unable to decode the Merkle Tree Item Hash.");
    while let Some(receiver) = stream_receivers.next().await {
        let shard_index = MerkleTreeConfiguration::tree_index(
            &receiver
                .0
                .try_into()
                .map_err(|_| crate::errors::IndexerError::BadUtxo)?
                .item_hash(&utxo_accumulator_item_hash, &mut ()),
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
4. Save every new shards to each corresponding shard index, and nullifier commitment.
*/
pub async fn sync_shards_from_full_node(
    ws_client: &WsClient,
    pool: &SqlitePool,
    max_count: (u64, u64),
    refresh_cache: bool,
) -> Result<()> {
    let mut current_checkpoint = crate::db::get_latest_check_point(pool).await?;
    let (max_sender_count, max_receiver_count) = max_count;

    let mut total_items = 0;
    let mut now = Instant::now();
    loop {
        let resp = synchronize_ledger(
            ws_client,
            &current_checkpoint,
            max_sender_count,
            max_receiver_count,
        )
        .await?;
        let shards = reconstruct_shards_from_pull_response(&resp).await?;

        // combine 3 updates into a transaction.
        let mut conn = pool.acquire().await?;
        let mut transaction_handler = conn.begin().await?;

        // update shards
        let mut stream_shards = tokio_stream::iter(shards.iter());
        while let Some((shard_index, shard)) = stream_shards.next().await {
            let utxo_index_beginning_offset =
                current_checkpoint.receiver_index[*shard_index as usize];
            for (offset, content) in shard.iter().enumerate() {
                let (utxo, note) = &content;
                crate::db::insert_one_shard(
                    &mut transaction_handler,
                    *shard_index,
                    (utxo_index_beginning_offset + offset) as u64,
                    utxo,
                    note,
                )
                .await?;
            }
        }

        // update nullifier
        let nullifier_checkpoint =
            crate::db::get_len_of_nullifier(&mut transaction_handler).await?;
        let mut stream_nullifiers = tokio_stream::iter(resp.senders.iter().enumerate());
        while let Some((idx, nullifier)) = stream_nullifiers.next().await {
            let i = idx + nullifier_checkpoint;
            crate::db::insert_one_nullifier(
                &mut transaction_handler,
                i as u64,
                &nullifier.0,
                &nullifier.1,
            )
            .await?;
        }

        // update total senders and receivers
        let new_total = resp.senders_receivers_total;
        crate::db::update_or_insert_total_senders_receivers(&mut transaction_handler, &new_total)
            .await?;

        transaction_handler.commit().await?;

        // update lru cache
        // ATTENTION: we should update cache only if we make sure the transaction has been committed successfully.
        if refresh_cache {
            // update receiver
            for (shard_index, items) in shards.iter() {
                let utxo_index_beginning_offset =
                    current_checkpoint.receiver_index[*shard_index as usize];
                let mut batch_receiver = Vec::with_capacity(items.len());
                for (offset, content) in items.iter().enumerate() {
                    batch_receiver.push((utxo_index_beginning_offset + offset, content.encode()));
                }
                put_batch_receiver(*shard_index, batch_receiver).await;
            }
            // update sender
            let mut batch_sender = Vec::with_capacity(resp.senders.len());
            for (idx, nullifier) in resp.senders.iter().enumerate() {
                batch_sender.push((nullifier_checkpoint + idx, nullifier.encode()));
            }
            put_batch_sender(batch_sender).await;
        }

        // update next checkpoint
        increasing_checkpoint(&mut current_checkpoint, resp.senders.len(), &shards);
        total_items += resp.receivers.len() + resp.senders.len();
        let time = now.elapsed().as_millis();
        now = Instant::now();
        debug!(
            target: "indexer",
            "sync new loop, fetch amount: {}, total amount: {}, total amount in server: {}, cost {} ms",
            resp.receivers.len(),
            total_items,
            <u128>::from_be_bytes(resp.senders_receivers_total),
            time
        );

        // should_continue == true means the synchronization is done.
        if !resp.should_continue {
            INIT_SYNCING_FINISHED.store(true, SeqCst);
            break;
        }
    }

    Ok(())
}

// When we start a indexer, we need to make sure the initialization syncing from full node
// is finished at least once before the service begins, so here's a disposable switch to indicate this scenario.
// Before we start the service to public, we check this variable and wait it becoming true.
// It will be set true once inner syncing loop found initialization done.
pub(crate) static INIT_SYNCING_FINISHED: AtomicBool = AtomicBool::new(false);
static SYNC_ERROR_COUNTER: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(indexer_ledger_opts(
        "sync_error_count",
        "syncing error counter"
    ))
    .expect("sync_error_count alloc fail")
});

pub fn start_sync_ledger_job(
    ws_client: WsClient,
    pool: SqlitePool,
    max_count: (u64, u64),
    frequency: u64,
    graceful_register: Option<tokio::sync::mpsc::Sender<()>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut synced_times = 0u32;
        loop {
            match sync_shards_from_full_node(&ws_client, &pool, (max_count.0, max_count.1), true)
                .await
            {
                Ok(_) => {
                    synced_times += 1;
                    if synced_times % 100 == 0 {
                        info!(
                            target: "indexer",
                            "synced utxo for {} times.",
                            synced_times
                        );
                    }
                    tokio::time::sleep(Duration::from_secs(frequency)).await;
                }
                Err(e) => {
                    error!(
                        target: "indexer",
                        "backend syncing job with err: {:?}",
                        e
                    );
                    SYNC_ERROR_COUNTER.inc();
                }
            }
            if SHUTDOWN_FLAG.load(SeqCst) {
                break;
            }
        }
        drop(graceful_register);
    })
}

#[cfg(test)]
pub async fn get_check_point_from_node(ws: &str) -> Result<Checkpoint> {
    let client = crate::utils::create_ws_client(ws).await?;

    let mut current_checkpoint = Checkpoint::default();
    let (max_sender_count, max_receiver_count) = (1024 * 15, 1024 * 15);

    let mut next_checkpoint = current_checkpoint;
    loop {
        let resp = synchronize_ledger(
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
            u128::from_le_bytes(resp.senders_receivers_total)
        );
        if !resp.should_continue {
            break;
        }
        current_checkpoint = next_checkpoint;
    }

    Ok(next_checkpoint)
}

/// According to the incremental `pull_ledger_diff` receiver and sender value,
/// move checkpoint cursor to next beginning offset.
fn increasing_checkpoint(
    checkpoint: &mut Checkpoint,
    incremental_sender: usize,
    incremental_receiver: &HashMap<u8, Vec<(Utxo, FullIncomingNote)>>,
) {
    checkpoint.sender_index += incremental_sender;
    incremental_receiver.iter().for_each(|(idx, utxos)| {
        checkpoint.receiver_index[*idx as usize] += utxos.len();
    });
}

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
        let resp = synchronize_ledger(
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
            u128::from_le_bytes(resp.senders_receivers_total)
        );
        if !resp.should_continue {
            break;
        }
        current_checkpoint = next_checkpoint;
        let t = now.elapsed().as_secs_f32();
        time_cost += t;
        times += 1;
        println!("pulling times: {times}");
    }
    Ok(time_cost)
}

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
        .await?
        .0;

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
        println!("pulling times: {times}. cost: {t}");
    }
    Ok(time_cost)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::cache::get_batch_shard_receiver;
    use crate::{db, types::*};
    use codec::{Decode, Encode};
    use manta_xt::dolphin_runtime::runtime_types::pallet_manta_pay::types::TransferPost;
    use manta_xt::{dolphin_runtime, utils, MantaConfig};
    use std::fs::File;
    use std::io::prelude::*;
    use std::io::BufReader;

    #[tokio::test]
    #[ignore]
    async fn mint_one_coin() {
        let config = crate::utils::read_config().unwrap();
        let port = config["indexer"]["configuration"]["port"]
            .as_integer()
            .unwrap();
        let indexer_address = format!("ws://127.0.0.1:{port}");
        let api = utils::create_manta_client::<MantaConfig>(&indexer_address)
            .await
            .expect("Failed to create client.");

        let file = File::open("./tests/integration-tests/precompile-coins/v1/precomputed_mints_v1")
            .unwrap();
        let mut buf_reader = BufReader::new(file);
        let mut contents = Vec::new();
        buf_reader.read_to_end(&mut contents).unwrap();
        let coin_size = 552;
        let off_set = 1;
        let start = off_set;

        let seed = "//Alice";
        let signer =
            utils::create_signer_from_string::<MantaConfig, manta_xt::sr25519::Pair>(seed).unwrap();

        let mut coin = &contents[start..start + coin_size];
        let mint = dolphin_runtime::tx()
            .manta_pay()
            .to_private(TransferPost::decode(&mut coin).unwrap());
        let block = api
            .tx()
            .sign_and_submit_then_watch_default(&mint, &signer)
            .await
            .unwrap()
            .wait_for_finalized() // for utxo syncing, we only query finalized chain state.
            .await
            .unwrap();
        let block_hash = block.block_hash();
        let _events = block.fetch_events().await.unwrap();
        println!("mint extrinsic submitted: {}", block_hash);
    }

    async fn mint_private_coins() {
        let config = crate::utils::read_config().unwrap();
        let port = config["indexer"]["configuration"]["port"]
            .as_integer()
            .unwrap();
        let indexer_address = format!("ws://127.0.0.1:{port}");
        let api = utils::create_manta_client::<MantaConfig>(&indexer_address)
            .await
            .expect("Failed to create client.");

        // this file contains 10 to_private extrinsics
        let file = File::open("./tests/integration-tests/precompile-coins/v1/precomputed_mints_v1")
            .unwrap();
        let mut buf_reader = BufReader::new(file);
        let mut contents = Vec::new();
        buf_reader.read_to_end(&mut contents).unwrap();
        let coin_size = 552;
        let off_set = 1;

        let seed = "//Alice";
        let signer =
            utils::create_signer_from_string::<MantaConfig, manta_xt::sr25519::Pair>(seed).unwrap();

        let batch_size = 5;
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

        let port = config["indexer"]["configuration"]["port"]
            .as_integer()
            .unwrap();
        let indexer_address = format!("ws://127.0.0.1:{port}");
        let api = utils::create_manta_client::<MantaConfig>(&indexer_address)
            .await
            .expect("Failed to create client.");

        let (max_sender_count, max_receiver_count) = (1024 * 10, 1024 * 10);
        let frequency = config["indexer"]["configuration"]["frequency"]
            .as_integer()
            .unwrap() as u64;
        let duplicated_pool = pool.clone();
        let handler = start_sync_ledger_job(
            client,
            duplicated_pool,
            (max_sender_count, max_receiver_count),
            frequency,
            None,
        );
        let last_check_point = db::get_latest_check_point(&pool).await.unwrap();

        // send 10 to_private extrinsics, 10 UTXOs should be generated.
        let expected_10_new_utxos = 10;
        mint_private_coins().await;

        // sleep 2 * frequency more seconds.
        tokio::time::sleep(Duration::from_secs(frequency * 2)).await;

        let current_check_point = db::get_latest_check_point(&pool).await.unwrap();

        let mut count = 0;
        for (shard_index, (last, current)) in last_check_point
            .receiver_index
            .iter()
            .zip(current_check_point.receiver_index.iter())
            .enumerate()
        {
            assert!(current >= last);
            if current > last {
                count += current - last;
                for i in *last..*current {
                    let _shard = dolphin_runtime::storage()
                        .manta_pay()
                        .shards(shard_index as u8, i as u64);
                    let shard = api
                        .storage()
                        .at(None)
                        .await
                        .unwrap()
                        .fetch(&_shard)
                        .await
                        .unwrap()
                        .unwrap();
                    let (utxo, note) = shard;
                    assert_ne!(utxo.encode(), Utxo::default().encode());
                    assert_ne!(note.encode(), FullIncomingNote::default().encode());
                }
            }
        }
        assert_eq!(count as u128, expected_10_new_utxos);
        assert!(count >= 1);
        handler.abort();
    }

    #[tokio::test]
    #[ignore]
    async fn bench_pull_ledger_diff_from_sqlite() {
        let migrations_path = "./migrations";
        let db_path = "tmp.db";
        let pool_size = 16u32;
        let pool = db::initialize_db_pool(migrations_path, db_path, pool_size).await;
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
        let indexer = "ws://127.0.0.1:9800";
        let times = 1;
        for _i in 0..times {
            i_sum += pull_ledger_diff_from_local_node(indexer).await.unwrap();
        }
        println!("indexer time cost: {} second", i_sum / times as f32);
    }

    #[tokio::test]
    async fn reconstruct_shards_should_be_correct() {
        let config = crate::utils::read_config().unwrap();
        let node = config["indexer"]["configuration"]["full_node"]
            .as_str()
            .unwrap();
        let client = crate::utils::create_ws_client(node).await.unwrap();

        let mut checkpoint = Checkpoint::default();
        checkpoint.sender_index = 0usize;
        let (max_sender_count, max_receiver_count) = (1024 * 6, 1024 * 6);

        let resp =
            synchronize_ledger(&client, &checkpoint, max_sender_count, max_receiver_count).await;
        let resp = resp.unwrap();
        let shards = reconstruct_shards_from_pull_response(&resp).await.unwrap();

        let api = utils::create_manta_client::<MantaConfig>(&node)
            .await
            .expect("Failed to create client.");

        let shard_idxs = [
            (31u8, 0u64),
            (123u8, 0u64),
            (60u8, 1u64),
            (167u8, 2u64),
            (136u8, 1u64),
        ];

        // check shards randomly for 5 times
        for i in 0..5usize {
            let shard_index = shard_idxs[i].0;
            let utxo_index = shard_idxs[i].1;
            let _shard = dolphin_runtime::storage()
                .manta_pay()
                .shards(shard_index as u8, utxo_index as u64);
            let shard = api
                .storage()
                .at(None)
                .await
                .unwrap()
                .fetch(&_shard)
                .await
                .unwrap()
                .unwrap();
            let (onchain_utxo, onchain_note) = shard;
            let reconstructed_utxo =
                &shards.get(&shard_index).as_ref().unwrap()[utxo_index as usize];

            assert_eq!(onchain_utxo.encode(), reconstructed_utxo.0.encode());
            assert_eq!(onchain_note.encode(), reconstructed_utxo.1.encode());
        }
    }

    #[tokio::test]
    #[ignore]
    async fn incremental_synchronization_should_update_cache() -> Result<()> {
        let pool = crate::db::create_test_db_or_first_pull(true).await;
        assert!(pool.is_ok());

        let indices: Vec<usize> = vec![1, 4, 7, 23, 32, 35, 39, 50, 88, 93];
        let rs = get_batch_shard_receiver(0, &indices).await;
        for r in rs {
            assert!(r.is_some());
        }
        Ok(())
    }
}
