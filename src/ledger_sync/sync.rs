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
use std::borrow::Cow;
use std::collections::HashMap;
use std::time::Instant;
use tokio_stream::StreamExt;

pub async fn synchronize_shards(
    ws: &WsClient,
    next_checkpoint: &Checkpoint,
    max_sender_count: u32,
    max_receiver_count: u32,
) -> Result<PullResponse> {
    let params = rpc_params![next_checkpoint, max_sender_count, max_receiver_count];

    let pull_response = ws
        .request::<PullResponse>("mantaPay_pull_ledger_diff", params)
        .await?;

    Ok(pull_response)
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
pub async fn sync_shards_from_full_node(pool: &SqlitePool, max_count: (u32, u32)) -> Result<()> {
    let url = "ws://127.0.0.1:9973";
    let client = crate::utils::create_ws_client(url).await?;

    let mut current_checkpoint = crate::db::get_latest_check_point(pool).await?;
    let (max_sender_count, max_receiver_count) = max_count;

    let mut next_checkpoint = current_checkpoint.clone();
    loop {
        let now = Instant::now();
        let resp = synchronize_shards(
            &client,
            &next_checkpoint,
            max_sender_count,
            max_receiver_count,
        )
        .await?;
        let shards = reconstruct_shards_from_pull_response(&resp)?;

        let mut stream_shards = tokio_stream::iter(shards.iter());
        while let Some((shard_index, shard)) = stream_shards.next().await {
            for (next_index, sh) in shard.iter().enumerate() {
                let encoded_utxo = sh.encode();
                crate::db::insert_a_single_shard(
                    pool,
                    *shard_index,
                    next_index as u64,
                    encoded_utxo,
                )
                .await?;
            }
        }

        let mut stream_vns = tokio_stream::iter(resp.senders.iter().enumerate());
        let vn_checkpoint = crate::db::get_len_of_void_number(pool).await?;
        while let Some((idx, vn)) = stream_vns.next().await {
            let encoded_vn = vn.encode();
            let i = idx + vn_checkpoint;
            crate::db::insert_a_single_void_number(pool, i as u64, encoded_vn).await?;
        }

        crate::ledger_sync::pull::calculate_next_checkpoint(
            &shards,
            &current_checkpoint,
            &mut next_checkpoint,
            resp.receivers.len(),
        );
        if !resp.should_continue {
            break;
        }
        current_checkpoint = next_checkpoint.clone();
    }

    Ok(())
}

/*
pub async fn pull_all_shards_to_db(pool: &SqlitePool) -> Result<()> {
    let url = "ws://127.0.0.1:9973";
    let client = crate::utils::create_ws_client(url).await.unwrap();

    let mut current_checkpoint = Checkpoint::default();
    current_checkpoint.sender_index = 0usize;
    let (max_sender_count, max_receiver_count) = (1024 * 15, 1024 * 15);

    let mut next_checkpoint = current_checkpoint.clone();
    let mut times = 0u32;
    let now = Instant::now();
    loop {
        let resp = synchronize_shards(
            &client,
            &next_checkpoint,
            max_sender_count,
            max_receiver_count,
        )
        .await
        .unwrap();
        let shards = reconstruct_shards_from_pull_response(&resp);

        let mut stream_shards = tokio_stream::iter(shards.iter());
        while let Some((shard_index, shard)) = stream_shards.next().await {
            for (next_index, sh) in shard.iter().enumerate() {
                let encoded_utxo = sh.encode();
                let row = crate::dn::insert_a_single_shard(pool, shard_index, next_index, encoded_utxo).await?;

            }
        }

        calculate_next_checkpoint(
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
        current_checkpoint = next_checkpoint.clone();
        times += 1;
        println!("times: {}", times);
    }
    let t = now.elapsed().as_secs();
    println!("time cost: {}", t);
}

pub async fn pull_ledger_diff_from_local_node() -> f32 {
    let url = "ws://127.0.0.1:9973";
    let client = crate::utils::create_ws_client(url).await.unwrap();

    let mut current_checkpoint = Checkpoint::default();
    current_checkpoint.sender_index = 0usize;
    let (max_sender_count, max_receiver_count) = (1024 * 15, 1024 * 15);

    let mut next_checkpoint = current_checkpoint.clone();
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
        .await
        .unwrap();
        let shards = reconstruct_shards_from_pull_response(&resp);

        calculate_next_checkpoint(
            &shards,
            &current_checkpoint,
            &mut next_checkpoint,
            resp.receivers.len(),
        );
        // println!("next checkpoint: {:?}, sender_index: {}, senders_receivers_total: {}", next_checkpoint, resp.receivers.len(), resp.senders_receivers_total);
        if !resp.should_continue {
            break;
        }
        current_checkpoint = next_checkpoint.clone();
        let t = now.elapsed().as_secs_f32();
        time_cost += t;
        times += 1;
        println!("pulling times: {}", times);
    }
    time_cost
}

pub fn pull_ledger_diff_from_sqlite() -> f32 {
    let db_path = "dolphin-shards.db";
    let conn = Connection::open(db_path).unwrap();

    let mut current_checkpoint = Checkpoint::default();
    current_checkpoint.sender_index = 0usize;
    let (max_sender_count, max_receiver_count) = (1024 * 15, 1024 * 15);

    let mut next_checkpoint = current_checkpoint.clone();
    let mut times = 0u32;
    let mut time_cost = 0f32;
    println!("========================");
    loop {
        let now = Instant::now();
        let resp = pull_ledger_diff(
            &conn,
            &next_checkpoint,
            max_sender_count,
            max_receiver_count,
        );
        // dbg!(&resp.receivers.len());

        let shards = reconstruct_shards_from_pull_response(&resp);
        // calculate_next_checkpoint(&shards, &current_checkpoint, &mut next_checkpoint, resp.receivers.len());
        calculate_next_checkpoint(&shards, &current_checkpoint, &mut next_checkpoint, 15360);
        // println!("next checkpoint: {:?}, sender_index: {}, senders_receivers_total: {}", next_checkpoint, resp.receivers.len(), resp.senders_receivers_total);
        if !resp.should_continue {
            break;
        }
        current_checkpoint = next_checkpoint.clone();
        let t = now.elapsed().as_secs_f32();
        time_cost += t;
        times += 1;
        println!("pulling times: {}", times);
    }
    time_cost
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use codec::Encode;
    use rusqlite::Connection;
    use std::mem;
    use std::time::{Duration, Instant};

    #[test]
    fn test_has_shard() {
        let db_path = "dolphin-shards.db";
        let conn = Connection::open(db_path).unwrap();

        dbg!(has_shard(&conn, 4, 2));

        dbg!(get_shard(&conn, 4, 2));
    }

    #[test]
    fn bench_pull_ledger_diff_from_sqlite() {
        let mut sum = 0f32;
        for i in 0..10 {
            sum += pull_ledger_diff_from_sqlite();
        }
        println!("time cost: {} second", sum / 10f32);
    }

    #[tokio::test]
    async fn bench_pull_shards_from_local_node() {
        let mut sum = 0f32;
        for i in 0..10 {
            sum += pull_ledger_diff_from_local_node().await;
        }
        println!("time cost: {} second", sum / 10f32);
    }

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

        let mut checkpoint = Checkpoint::default();
        checkpoint.sender_index = 0usize;
        let (max_sender_count, max_receiver_count) = (1024 * 100, 1024 * 100);

        let resp =
            synchronize_shards(&client, &checkpoint, max_sender_count, max_receiver_count).await;
        assert!(&resp.is_ok());
        assert!(resp.unwrap().should_continue);
    }

    #[tokio::test]
    async fn pull_all_shards_to_db() {
        let db_path = "shards.db";

        // let conn = crate::utils::open_db(db_path).await.unwrap();
        let conn = Connection::open(db_path).unwrap();

        // create table
        let table = "
            CREATE TABLE shards (
                shard_index   INTEGER NOT NULL,
                next_index  INTEGER NOT NULL,
                utxo    BLOB NOT NULL
            );
        ";
        assert!(conn.execute(table, ()).is_ok());

        // create index on shards
        let shards_index = "
            CREATE INDEX shard_index ON shards (
                shard_index	ASC,
                next_index
            );
        ";
        assert!(conn.execute(shards_index, ()).is_ok());

        let url = "ws://127.0.0.1:9973";
        let client = crate::utils::create_ws_client(url).await.unwrap();

        let mut current_checkpoint = Checkpoint::default();
        current_checkpoint.sender_index = 0usize;
        let (max_sender_count, max_receiver_count) = (1024 * 15, 1024 * 15);
        // let (max_sender_count, max_receiver_count) = (4, 4);

        let mut next_checkpoint = current_checkpoint.clone();
        let mut times = 0u32;
        let now = Instant::now();
        loop {
            let resp = synchronize_shards(
                &client,
                &next_checkpoint,
                max_sender_count,
                max_receiver_count,
            )
            .await
            .unwrap();
            let shards = reconstruct_shards_from_pull_response(&resp);

            let mut stream_shards = tokio_stream::iter(shards.iter());
            // INSERT INTO shards (shard_index, next_index, utxo) VALUES (?1, ?2, ?3)
            while let Some((shard_index, shard)) = stream_shards.next().await {
                for (next_index, sh) in shard.iter().enumerate() {
                    let encoded_shard = sh.encode();
                    conn.execute(
                        "INSERT INTO shards (shard_index, next_index, utxo) VALUES (?1, ?2, ?3)",
                        rusqlite::params![shard_index, next_index, encoded_shard],
                    )
                    .unwrap();
                }
            }

            calculate_next_checkpoint(
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
            current_checkpoint = next_checkpoint.clone();
            times += 1;
            println!("times: {}", times);
        }
        let t = now.elapsed().as_secs();
        println!("time cost: {}", t);
    }

    #[tokio::test]
    async fn reconstruct_shards_should_be_correct() {
        let url = "wss://ws.rococo.dolphin.engineering:443";
        let client = crate::utils::create_ws_client(url).await.unwrap();

        let mut checkpoint = Checkpoint::default();
        checkpoint.sender_index = 0usize;
        let (max_sender_count, max_receiver_count) = (1024, 1024);

        let resp =
            synchronize_shards(&client, &checkpoint, max_sender_count, max_receiver_count).await;
        // assert!(&resp.is_ok());
        let resp = resp.unwrap();
        assert!(resp.should_continue);
        let shards = reconstruct_shards_from_pull_response(&resp);

        let length = shards.len();
        for (k, shard) in shards.iter() {
            for sh in shard.iter() {
                // if *k as usize + 1 >= length {
                println!(
                    "k: {}, utxo: {:?}, ephemeral_public_key: {:?}, ciphertext: {:?}",
                    k,
                    hex::encode(sh.0),
                    hex::encode(sh.1.ephemeral_public_key),
                    hex::encode(sh.1.ciphertext)
                );
                // }
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

        // println!("---------------");
        // let next_checkpoint = calculate_next_checkpoint(&shards, &checkpoint, resp.receivers.len());
        // dbg!(&next_checkpoint);
        // let resp =
        //     synchronize_shards(&client, &next_checkpoint, max_sender_count, max_receiver_count).await;
        // // assert!(&resp.is_ok());
        // let resp = resp.unwrap();
        // assert!(resp.should_continue);
        // let shards = reconstruct_shards_from_pull_response(&resp);

        // let length = shards.len();
        // for (k, shard) in shards.iter() {
        //     for sh in shard.iter() {
        //         println!(
        //             "k: {}, utxo: {:?}, ephemeral_public_key: {:?}, ciphertext: {:?}",
        //             k,
        //             hex::encode(sh.0),
        //             hex::encode(sh.1.ephemeral_public_key),
        //             hex::encode(sh.1.ciphertext)
        //         );
        //     }
        // }
    }

    #[tokio::test]
    async fn get_storage_hash_should_work() {
        let url = "wss://ws.rococo.dolphin.engineering:443";
        let client = crate::utils::create_ws_client(url).await.unwrap();
        let hash = get_storage_hash(&client).await;
        assert!(hash.is_ok());
    }

    #[tokio::test]
    async fn write_shards_to_db() {
        // open db
        let db_path = "shards.db";

        // let conn = crate::utils::open_db(db_path).await.unwrap();
        let conn = Connection::open(db_path).unwrap();
        // init db
        // dbg!(db::initialize_db(conn.clone()).await);

        // create table
        let table = "
            CREATE TABLE shards (
                shard_index   INTEGER NOT NULL,
                next_index  INTEGER NOT NULL,
                utxo    BLOB NOT NULL
            );
        ";
        // dbg!(conn.execute(table, ()));

        // create index on shards
        let shards_index = "
            CREATE INDEX shard_index ON shards (
                shard_index	ASC,
                next_index
            );
        ";
        // dbg!(conn.execute(shards_index, ()));

        // let url = "ws://127.0.0.1:9973";
        // let client = crate::utils::create_ws_client(url).await.unwrap();
        // let resp = synchronize_shards(&client).await;
        // let resp = resp.unwrap();
        // assert!(resp.should_continue);
        // dbg!(resp.senders_receivers_total);
        // dbg!(resp.receivers.len());
        // dbg!(resp.senders.len());
        // let shards = reconstruct_shards_from_pull_response(&resp);
        // assert!(shards.is_ok());
        // let shards = shards.unwrap();

        // let mut stream_shards = tokio_stream::iter(shards.into_iter());

        // // INSERT INTO shards (shard_index, next_index, utxo) VALUES (?1, ?2, ?3)
        // while let Some((shard_index, shard)) = stream_shards.next().await {
        //     for (next_index, sh) in shard.iter().enumerate() {
        //         let encoded_shard = sh.encode();
        //         conn.execute(
        //                 "INSERT INTO shards (shard_index, next_index, utxo) VALUES (?1, ?2, ?3)",
        //                 rusqlite::params![shard_index, next_index, encoded_shard],
        //             )
        //             .unwrap();
        //     }
        // }

        // let query_shard = "SELECT shard_index, next_index, utxo FROM Shards WHERE shard_index >= 8 and shard_index <= 162"
        // let smt: Result<Shard, _> = conn.query_map("SELECT shard_index, next_index, utxo FROM shards WHERE shard_index = ?1", [0], |row| row.get(0));
        // let mut stmt = conn.prepare("SELECT shard_index, next_index, utxo FROM shards WHERE shard_index >= (?1) and next_index = (?2)").unwrap();
        let mut stmt = conn.prepare("SELECT shard_index, next_index, utxo FROM shards WHERE shard_index >= (?1) and next_index = (?2)").unwrap();
        let mut stmt = conn
            .prepare("SELECT shard_index, next_index, utxo FROM shards WHERE shard_index >= (?1)")
            .unwrap();
        let mut ms = 0u128;
        for i in 0..5u8 {
            let now = Instant::now();
            let sh = stmt
                .query_map([0], |row| {
                    Ok(Shard {
                        shard_index: row.get(0)?,
                        next_index: row.get(1)?,
                        utxo: row.get(2)?,
                    })
                })
                .unwrap();
            // for s in sh {
            //     let s = s.unwrap();
            //     // dbg!(mem::size_of_val(&s.shard_index));
            //     // dbg!(mem::size_of_val(&s.next_index));
            //     // dbg!(mem::size_of_val(&s.utxo));
            //     // dbg!(mem::size_of_val(&s));
            // }
            let t = now.elapsed().as_nanos();
            if i > 0 {
                ms += t;
            }
            println!("length: {}, time: {}", sh.count(), t);
        }
        println!("{}", ms / 4);
    }
}
*/
