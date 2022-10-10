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
use crate::types::{EncryptedNote, PullResponse, ReceiverChunk, Shard, Utxo};
use crate::utils;
use anyhow::Result;
use codec::{Decode, Encode};
use frame_support::storage::storage_prefix;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use manta_crypto::merkle_tree::forest::Configuration;
use manta_pay::config::MerkleTreeConfiguration;
use manta_pay::signer::Checkpoint;
use rusqlite::Connection;
use sp_core::storage::StorageData;
use std::borrow::Cow;
use std::collections::HashMap;
use std::time::{Duration, Instant};
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
) -> HashMap<u8, Vec<(Utxo, EncryptedNote)>> {
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
    shards
}

pub fn calculate_next_checkpoint(
    previous_shards: &HashMap<u8, Vec<(Utxo, EncryptedNote)>>,
    previous_checkpoint: &Checkpoint,
    next_checkpoint: &mut Checkpoint,
    sender_index: usize,
) {
    // point to next void number
    next_checkpoint.sender_index += sender_index;
    for (i, utxos) in previous_shards.iter() {
        let index = *i as usize;
        // offset for next shard
        next_checkpoint.receiver_index[index] =
            previous_checkpoint.receiver_index[index] + utxos.len()
    }
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

pub fn pull_receivers(
    conn: &Connection,
    receiver_indices: [usize; 256],
    max_update_request: u64,
) -> (bool, ReceiverChunk) {
    let mut more_receivers = false;
    let mut receivers = Vec::new();
    let mut receivers_pulled: u64 = 0;
    let max_update = if max_update_request > PULL_MAX_RECEIVER_UPDATE_SIZE {
        PULL_MAX_RECEIVER_UPDATE_SIZE
    } else {
        max_update_request
    };

    for (shard_index, utxo_index) in receiver_indices.into_iter().enumerate() {
        more_receivers |= pull_receivers_for_shard(
            conn,
            shard_index as u8,
            utxo_index,
            max_update,
            &mut receivers,
            &mut receivers_pulled,
        );
        if receivers_pulled == max_update && more_receivers {
            break;
        }
    }
    (more_receivers, receivers)
}

fn has_shard(conn: &Connection, shard_index: u8, next_index: u64) -> bool {
    let mut stmt = conn.prepare("SELECT shard_index, next_index FROM shards WHERE shard_index = (?) and next_index = (?)").unwrap();
    let result: rusqlite::Result<u8, _> =
        stmt.query_row(rusqlite::params![shard_index, next_index], |row| row.get(0));
    // let result: rusqlite::Result<u8, _> = conn.execute("SELECT shard_index, next_index FROM shards WHERE shard_index = (?) and next_index = (?)", [shard_index, next_index])
    if result.is_err() {
        return false;
    }
    result.unwrap() == shard_index
}

// fn get_shard(conn: &Connection, shard_index: u8, next_index: u64) -> Result<(Utxo, EncryptedNote)> {
fn get_shard(conn: &Connection, shard_index: u8, next_index: u64) -> rusqlite::Result<Vec<u8>> {
    let mut stmt = conn.prepare("SELECT shard_index, next_index, utxo FROM shards WHERE shard_index = (?) and next_index = (?)")?;
    let shard: rusqlite::types::Value =
        stmt.query_row(rusqlite::params![shard_index, next_index], |row| row.get(2))?;
    match shard {
        rusqlite::types::Value::Blob(sh) => Ok(sh),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

// SELECT shard_index, next_index, utxo FROM shards WHERE shard_index BETWEEN (?1) and (?2) and next_index BETWEEN (?3) and (?4)
// fn get_batch_shard(
//     conn: &Connection,
//     receivers: &mut ReceiverChunk,
//     shard_index: u8,
//     range: (u64, u64),
//     receivers_pulled: &mut u64,
//     max_update: u64,
// ) -> rusqlite::Result<Vec<(Utxo, EncryptedNote)>> {
//     let mut stmt = conn.prepare("SELECT shard_index, next_index, utxo FROM shards WHERE shard_index = (?1) and next_index BETWEEN (?2) and (?3)")?;
//     let mut shards = stmt.query(rusqlite::params![shard_index, range.0, range.1])?;

//     // let mut utxos = Vec::with_capacity(shards.map(|_| Ok(())).count()?);
//     let mut idx = range.0;
//     while let Some(row) = shards.next()? {
//         if *receivers_pulled == max_update {
//             return has_shard(conn, shard_index, idx);
//         }
//         match row.get(2)? {
//             rusqlite::types::Value::Blob(sh) => {
//                 *receivers_pulled += 1;
//                 let mut a = sh.as_slice();
//                 // dbg!(&receivers_pulled);
//                 let n: (Utxo, EncryptedNote) =
//                     <(Utxo, EncryptedNote) as Decode>::decode(&mut a).unwrap();
//                 receivers.push(n);
//             }
//             _ => todo!(),
//         }
//         idx += 1;
//     }

//     Ok(utxos)
// }

pub fn pull_receivers_for_shard(
    conn: &Connection,
    shard_index: u8,
    receiver_index: usize,
    max_update: u64,
    receivers: &mut ReceiverChunk,
    receivers_pulled: &mut u64,
) -> bool {
    let max_receiver_index = (receiver_index as u64) + max_update;
    // for idx in (receiver_index as u64)..max_receiver_index {
    //     if *receivers_pulled == max_update {
    //         return has_shard(conn, shard_index, idx);
    //     }
    // match get_shard(conn, shard_index, idx) {
    //     Ok(mut next) => {
    //         *receivers_pulled += 1;
    //         let mut a = next.as_slice();
    //         let n: (Utxo, EncryptedNote) = <(Utxo, EncryptedNote) as Decode>::decode(&mut a).unwrap();
    //         receivers.push(n);
    //     }
    //     _ => return false,
    // }
    // }
    // get_batch_shard(
    //     conn,
    //     receivers,
    //     shard_index,
    //     (receiver_index as u64, max_receiver_index - 1),
    //     receivers_pulled,
    //     max_update,
    // )
    // .unwrap();
    let mut stmt = conn.prepare("SELECT shard_index, next_index, utxo FROM shards WHERE shard_index = (?1) and next_index BETWEEN (?2) and (?3)").unwrap();
    let mut shards = stmt
        .query(rusqlite::params![
            shard_index,
            receiver_index,
            max_receiver_index
        ])
        .unwrap();

    let mut idx = receiver_index;
    while let Some(row) = shards.next().unwrap() {
        if *receivers_pulled == max_update {
            return has_shard(conn, shard_index, idx as u64);
        }
        match row.get(2).unwrap() {
            rusqlite::types::Value::Blob(sh) => {
                *receivers_pulled += 1;
                let mut a = sh.as_slice();
                // dbg!(&receivers_pulled);
                let n: (Utxo, EncryptedNote) =
                    <(Utxo, EncryptedNote) as Decode>::decode(&mut a).unwrap();
                receivers.push(n);
            }
            _ => todo!(),
        }
        idx += 1;
    }
    has_shard(conn, shard_index, max_receiver_index)
}

pub fn pull_ledger_diff(
    conn: &Connection,
    checkpoint: &Checkpoint,
    max_receivers: u64,
    max_senders: u64,
) -> PullResponse {
    let (more_receivers, receivers) =
        pull_receivers(conn, *checkpoint.receiver_index, max_receivers);
    // let (more_senders, senders) = Self::pull_senders(checkpoint.sender_index, max_senders);
    // let senders_receivers_total = (0..=255)
    //     .map(|i| ShardTrees::<T>::get(i).current_path.leaf_index as u128)
    //     .sum::<u128>()
    //     + VoidNumberSetSize::<T>::get() as u128;
    let senders = Default::default();
    let senders_receivers_total = 162584;
    PullResponse {
        should_continue: more_receivers,
        receivers,
        senders,
        senders_receivers_total,
    }
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
