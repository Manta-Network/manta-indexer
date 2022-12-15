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

use crate::types::{
    Checkpoint, FullIncomingNote, Nullifier, NullifierCommitment, OutgoingNote, SenderChunk, Shard,
    Utxo,
};
use anyhow::Result;
use codec::{Decode, Encode};
use sqlx::{
    migrate::{MigrateDatabase, Migrator},
    sqlite::{SqlitePool, SqlitePoolOptions},
    Error as SqlxError, Row,
};
use std::path::Path;
use tokio_stream::StreamExt;

/// Initialize sqlite as WAL mode(Write-Ahead Logging)
// https://sqlite.org/wal.html
pub async fn initialize_db_pool(db_url: &str, pool_size: u32) -> Result<SqlitePool> {
    let m = Migrator::new(Path::new("./migrations")).await?;

    // ensure the db exists.
    if let Err(_) | Ok(false) = sqlx::sqlite::Sqlite::database_exists(db_url).await {
        sqlx::sqlite::CREATE_DB_WAL.store(true, std::sync::atomic::Ordering::Release);
        assert!(sqlx::sqlite::Sqlite::create_database(db_url).await.is_ok());
    }

    let pool = SqlitePoolOptions::new()
        .max_lifetime(None)
        .idle_timeout(None)
        .max_connections(pool_size)
        .connect(db_url)
        .await?;

    let mut conn = pool.acquire().await?;
    m.run(&mut conn).await?;

    sqlx::query(r#"PRAGMA synchronous = OFF;"#)
        .execute(&pool)
        .await?;

    // Enable WAL mode
    sqlx::query(r#"PRAGMA synchronous = OFF;"#)
        .execute(&pool)
        .await?;

    Ok(pool)
}

/// Whether the sharded item exists in the db or not.
pub async fn has_shard(pool: &SqlitePool, shard_index: u8, utxo_index: u64) -> bool {
    let n = utxo_index as i64;

    let one = sqlx::query("SELECT * FROM shards WHERE shard_index = ?1 and utxo_index = ?2;")
        .bind(shard_index)
        .bind(n)
        .fetch_one(pool)
        .await;

    match one {
        Ok(_) => true,
        Err(SqlxError::RowNotFound) => false,
        Err(_) => false, // this case should happen, in theory
    }
}

/// Get a single shard from db.
pub async fn get_one_shard(pool: &SqlitePool, shard_index: u8, utxo_index: u64) -> Result<Shard> {
    let n = utxo_index as i64;

    let one = sqlx::query_as("SELECT * FROM shards WHERE shard_index = ?1 and utxo_index = ?2;")
        .bind(shard_index)
        .bind(n)
        .fetch_one(pool)
        .await;

    one.map_err(From::from)
}

/// Get a batched shard from db.
/// [utxo_index_beginning, utxo_index_ending), including the start, not including the end.
pub async fn get_batched_shards(
    pool: &SqlitePool,
    shard_index: u8,
    utxo_index_beginning: u64,
    utxo_index_ending: u64,
) -> Result<Vec<Shard>> {
    Ok(sqlx::query_as(
        "SELECT * FROM shards WHERE shard_index = ?1 and utxo_index >= ?2 and utxo_index < ?3;",
    )
    .bind(shard_index)
    .bind(utxo_index_beginning as i64)
    .bind(utxo_index_ending as i64)
    .fetch_all(pool)
    .await?)
}

pub async fn get_batched_shards_by_idxs<'a, E>(
    executor: E,
    shard_index: u8,
    utxos_idxs: Vec<i64>,
) -> Result<Vec<Shard>>
where
    E: sqlx::sqlite::SqliteExecutor<'a>,
{
    if utxos_idxs.is_empty() {
        return Ok(vec![]);
    }
    let params = (1..=utxos_idxs.len())
        .map(|i| format!("?{}", i + 1))
        .collect::<Vec<String>>()
        .join(",");
    let sql = format!("SELECT * FROM shards WHERE shard_index = ?1 and utxo_index IN ({params})");

    let mut query = sqlx::query_as(sql.as_str());
    query = query.bind(shard_index);
    for idx in utxos_idxs {
        query = query.bind(idx);
    }
    Ok(query.fetch_all(executor).await?)
}

pub async fn insert_one_shard<'a, E>(
    executor: E,
    shard_index: u8,
    utxo_index: u64,
    utxo: &Utxo,
    note: &FullIncomingNote,
) -> Result<()>
where
    E: sqlx::sqlite::SqliteExecutor<'a>,
{
    sqlx::query("INSERT OR REPLACE INTO shards (shard_index, utxo_index, utxo, full_incoming_note) VALUES (?1, ?2, ?3, ?4);")
            .bind(shard_index)
            .bind(utxo_index as i64)
            .bind(utxo.encode())
            .bind(note.encode())
            .execute(executor)
            .await?;
    Ok(())
}

pub async fn insert_one_nullifier<'a, E>(
    executor: E,
    nullifier_index: u64,
    nc: &NullifierCommitment,
    on: &OutgoingNote,
) -> Result<()>
where
    E: sqlx::sqlite::SqliteExecutor<'a>,
{
    sqlx::query(
        "INSERT OR REPLACE INTO nullifier (idx, nullifier_commitment, outgoing_note) VALUES (?1, ?2, ?3);",
    )
    .bind(nullifier_index as i64)
    .bind(nc.encode())
    .bind(on.encode())
    .execute(executor)
    .await?;
    Ok(())
}

/// Instead of insert with specific idx, just append a new nullifier record with auto increasing idx primary key.
pub async fn append_nullifier<'a, E>(
    executor: E,
    nc: &NullifierCommitment,
    on: &OutgoingNote,
) -> Result<()>
where
    E: sqlx::sqlite::SqliteExecutor<'a>,
{
    sqlx::query("INSERT INTO nullifier (nullifier_commitment, outgoing_note) VALUES (?1, ?2)")
        .bind(nc.encode())
        .bind(on.encode())
        .execute(executor)
        .await?;
    Ok(())
}

pub async fn get_len_of_nullifier<'a, E>(executor: E) -> Result<usize>
where
    E: sqlx::sqlite::SqliteExecutor<'a>,
{
    // TODO. deal with hole.
    Ok(sqlx::query(r#"SELECT count(*) as count FROM nullifier;"#)
        .fetch_one(executor)
        .await?
        .get::<u32, _>("count") as usize)
}

pub async fn get_latest_check_point(pool: &SqlitePool) -> Result<Checkpoint> {
    let mut ckp = Checkpoint::default();

    let mut stream = tokio_stream::iter(0..=255u8);
    // If there's no any shard in db, return default checkpoint.
    while let Some(shard_index) = stream.next().await {
        // TODO deal with hole.
        let count_of_utxo: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM shards WHERE shard_index = ?;")
                .bind(shard_index)
                .fetch_one(pool)
                .await?;

        ckp.receiver_index[shard_index as usize] = count_of_utxo.0 as usize;
    }
    ckp.sender_index = get_len_of_nullifier(pool).await?;

    Ok(ckp)
}

/// Whether the nullifier exists in the db or not.
pub async fn has_nullifier(pool: &SqlitePool, nullifier_index: u64) -> bool {
    let n = nullifier_index as i64;
    let one = sqlx::query("SELECT nullifier_commitment FROM nullifier WHERE idx = ?1;")
        .bind(n)
        .fetch_one(pool)
        .await;

    match one {
        Ok(_) => true,
        Err(SqlxError::RowNotFound) => false,
        Err(_) => false, // this case should happen, in theory
    }
}

pub async fn get_one_nullifier(
    pool: &SqlitePool,
    nullifier_index: u64,
) -> Result<(NullifierCommitment, OutgoingNote)> {
    let n = nullifier_index as i64;
    let one_row = sqlx::query("SELECT * FROM nullifier WHERE idx = ?1;")
        .bind(n)
        .fetch_one(pool)
        .await?;
    let nc: NullifierCommitment = one_row
        .get::<Vec<u8>, _>("nullifier_commitment")
        .try_into()
        .map_err(|_| {
            crate::errors::IndexerError::DecodedError("NullifierCommitment".to_string())
        })?;

    let mut _on = one_row.get::<&[u8], _>("outgoing_note");
    let on = <OutgoingNote as Decode>::decode(&mut _on)?;

    Ok((nc, on))
}

/// Fetch a batch of nullifier data which index range is [beginning, ending) (without ending).
pub async fn get_batched_nullifier(
    pool: &SqlitePool,
    nullifier_beginning: u64,
    nullifier_ending: u64,
) -> Result<SenderChunk> {
    let batched_nullifiers = sqlx::query(
        "SELECT nullifier_commitment, outgoing_note FROM nullifier WHERE idx >= ?1 and idx < ?2;",
    )
    .bind(nullifier_beginning as i64)
    .bind(nullifier_ending as i64)
    .fetch_all(pool)
    .await?;
    let mut nullifiers = Vec::with_capacity(batched_nullifiers.len());
    for i in &batched_nullifiers {
        let nc: NullifierCommitment = i
            .get::<Vec<u8>, _>("nullifier_commitment")
            .try_into()
            .map_err(|_| {
                crate::errors::IndexerError::DecodedError("nullifier_commitment".to_string())
            })?;

        let mut _on = i.get::<&[u8], _>("outgoing_note");
        let on = <OutgoingNote as Decode>::decode(&mut _on)?;
        nullifiers.push((nc, on))
    }

    Ok(nullifiers)
}

pub async fn get_batched_nullifier_by_idxs<'a, E>(
    executor: E,
    indices: Vec<i64>,
) -> Result<Vec<Nullifier>>
where
    E: sqlx::sqlite::SqliteExecutor<'a>,
{
    if indices.is_empty() {
        return Ok(vec![]);
    }
    let params = (1..=indices.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<String>>()
        .join(",");
    let sql = format!("SELECT * FROM nullifier WHERE idx IN ({params})");

    let mut query = sqlx::query_as(sql.as_str());
    for idx in indices {
        query = query.bind(idx);
    }
    Ok(query.fetch_all(executor).await?)
}

pub async fn update_or_insert_total_senders_receivers<'a, E>(
    executor: E,
    new_total: &[u8; 16],
) -> Result<()>
where
    E: sqlx::sqlite::SqliteExecutor<'a>,
{
    sqlx::query("INSERT OR REPLACE INTO senders_receivers_total (total) VALUES (?1);")
        .bind(new_total.to_vec())
        .execute(executor)
        .await?;
    Ok(())
}

pub async fn get_total_senders_receivers(pool: &SqlitePool) -> Result<[u8; 16]> {
    let total: (Vec<u8>,) = sqlx::query_as("SELECT total FROM senders_receivers_total;")
        .fetch_one(pool)
        .await?;

    let v: [u8; 16] = total
        .0
        .try_into()
        .map_err(|_| crate::errors::IndexerError::WrongLittleEndianArray)?;
    Ok(v)
}

#[cfg(test)]
async fn clean_up(conn: &mut SqlitePool, table_name: &str) -> Result<()> {
    use sqlx::Executor;

    let going_to_drop = format!("DROP TABLE {}", table_name);
    conn.execute(&*going_to_drop).await.ok();

    Ok(())
}

#[cfg(test)]
pub async fn create_test_db_or_first_pull(is_tmp: bool) -> Result<SqlitePool> {
    let config = crate::utils::read_config().unwrap();
    let node = config["indexer"]["configuration"]["full_node"]
        .as_str()
        .unwrap();

    let db_path = if is_tmp {
        let tmp_db = tempfile::Builder::new()
            .prefix("tmp-utxo")
            .suffix(&".db")
            .tempfile()
            .unwrap();
        let db_path = tmp_db.path().to_str().unwrap();
        db_path.to_string()
    } else {
        let db_path = config["db"]["configuration"]["db_path"].as_str().unwrap();
        db_path.to_string()
    };
    let pool_size = 16u32;
    let pool = initialize_db_pool(&db_path, pool_size).await?;

    // if the db is empty, pull utxos.
    if !has_shard(&pool, 0, 0).await {
        let ws_client = crate::utils::create_ws_client(node).await?;
        crate::indexer::sync::sync_shards_from_full_node(&ws_client, &pool, (1024, 1024), true)
            .await?;
    }

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FullIncomingNote, PullResponse, Shard};
    use codec::Encode;
    use rand::distributions::{Distribution, Uniform};
    use rand::prelude::SliceRandom;

    #[tokio::test]
    async fn do_migration_should_work() {
        let pool = create_test_db_or_first_pull(true).await;
        assert!(pool.is_ok());
    }

    #[tokio::test]
    async fn get_one_shard_should_work() {
        let pool = create_test_db_or_first_pull(true).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        let shard_index = 206u8;
        let utxo_index = 1;
        let one_shard = get_one_shard(&pool, shard_index, utxo_index).await;

        assert!(one_shard.is_ok());
        assert_eq!(one_shard.as_ref().unwrap().shard_index, shard_index);
    }

    #[tokio::test]
    async fn has_shard_should_work() {
        let pool = create_test_db_or_first_pull(true).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        let shard_index = 206u8;
        let utxo_index = 1;
        assert!(has_shard(&pool, shard_index, utxo_index).await);

        let invalid_utxo_index = u64::MAX;

        // This shard should not exist.
        assert!(!has_shard(&pool, shard_index, invalid_utxo_index).await);
    }

    #[tokio::test]
    async fn get_batched_shards_should_work() {
        let pool = create_test_db_or_first_pull(true).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        let shard_index = 206u8;
        let utxo_index_between = Uniform::from(0..2);
        let mut rng = rand::thread_rng();
        let mut from_utxo_index = utxo_index_between.sample(&mut rng);
        let mut to_utxo_index = utxo_index_between.sample(&mut rng);

        if to_utxo_index < from_utxo_index {
            std::mem::swap(&mut to_utxo_index, &mut from_utxo_index);
        }

        let batched_shards =
            get_batched_shards(&pool, shard_index, from_utxo_index, to_utxo_index).await;

        assert!(batched_shards.is_ok());

        let batched_shards = batched_shards.unwrap();
        assert_eq!(
            batched_shards.len(),
            (to_utxo_index - from_utxo_index) as usize
        );
    }

    #[tokio::test]
    #[ignore]
    async fn get_batched_shards_by_idxs_should_work() {
        // SimpleLogger::new().with_level(Trace).init().unwrap();
        let pool = create_test_db_or_first_pull(true).await;
        assert!(pool.is_ok(), "{:?}", pool.err().unwrap());

        let pool = pool.unwrap();

        let shard_index_between = Uniform::from(0..=255); // [0, 256)
        let utxo_index_between = Uniform::from(0..50);
        let mut rng = rand::thread_rng();
        let shard_index = shard_index_between.sample(&mut rng);
        let mut from_utxo_index = utxo_index_between.sample(&mut rng);
        let mut to_utxo_index = utxo_index_between.sample(&mut rng);

        if to_utxo_index < from_utxo_index {
            std::mem::swap(&mut to_utxo_index, &mut from_utxo_index);
        }
        assert!(to_utxo_index > from_utxo_index);

        let mut idxs = (from_utxo_index..=to_utxo_index).collect::<Vec<i64>>();
        idxs.shuffle(&mut rng);
        idxs.drain((idxs.len() / 2)..);

        let batched_shards = get_batched_shards_by_idxs(&pool, shard_index, idxs.clone()).await;

        assert!(
            batched_shards.is_ok(),
            "{:?}",
            batched_shards.err().unwrap()
        );

        let batched_shards = batched_shards.unwrap();
        assert_eq!(batched_shards.len(), idxs.len());
        for item in batched_shards {
            assert!(
                idxs.contains(&item.utxo_index),
                "{} not in {:?}",
                item.utxo_index,
                idxs
            );
        }
    }

    #[tokio::test]
    async fn insert_one_shard_should_work() {
        let tmp_db = tempfile::Builder::new()
            .prefix("tmp-utxo")
            .suffix(&".db")
            .tempfile()
            .unwrap();
        let db_url = tmp_db.path().to_str().unwrap();

        let pool = initialize_db_pool(db_url, 16).await;
        assert!(pool.is_ok());

        let mut pool = pool.unwrap();

        let utxo = Utxo {
            is_transparent: false,
            ..Default::default()
        };
        let note = FullIncomingNote {
            address_partition: 1,
            ..Default::default()
        };
        let shard_index = 0u8;
        let utxo_index = 0u64;
        assert!(
            insert_one_shard(&pool, shard_index, utxo_index, &utxo, &note)
                .await
                .is_ok()
        );

        let shard = get_one_shard(&pool, shard_index, utxo_index).await;
        assert!(shard.is_ok());
        let shard = shard.unwrap();

        let orignal_shard = Shard {
            shard_index,
            utxo_index: utxo_index as i64,
            utxo: utxo.encode(),
            full_incoming_note: note.encode(),
        };

        assert!(clean_up(&mut pool, "shards").await.is_ok());
        assert_eq!(orignal_shard, shard);
    }

    #[tokio::test]
    async fn get_latest_check_point_should_work() {
        let pool = create_test_db_or_first_pull(true).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        let ckp_from_db = get_latest_check_point(&pool).await.unwrap();

        // get check point from full node
        let config = crate::utils::read_config().unwrap();
        let node = config["indexer"]["configuration"]["full_node"]
            .as_str()
            .unwrap();
        let ckp_from_node = crate::indexer::sync::get_check_point_from_node(node)
            .await
            .unwrap();

        // compare both check points
        assert_eq!(ckp_from_db.sender_index, ckp_from_node.sender_index);
        assert_eq!(ckp_from_db.receiver_index, ckp_from_node.receiver_index);
    }

    #[tokio::test]
    async fn insert_nullifier_should_work() {
        let tmp_db = tempfile::Builder::new()
            .prefix("tmp-utxo")
            .suffix(&".db")
            .tempfile()
            .unwrap();
        let db_url = tmp_db.path().to_str().unwrap();
        let pool = initialize_db_pool(db_url, 16).await;
        assert!(pool.is_ok());
        let pool = pool.unwrap();

        let i = 1;
        let nc = [1u8; 32];
        let on = OutgoingNote::default();
        assert!(insert_one_nullifier(&pool, i, &nc, &on).await.is_ok());

        // query one nullifier commitment
        let _nc = get_one_nullifier(&pool, i).await;
        assert_eq!(_nc.unwrap(), (nc, on));

        let (start, end) = (2u64, 6u64);
        for i in start..=end {
            let new_nc = [i as u8; 32];
            let new_on = OutgoingNote {
                ephemeral_public_key: [i as u8; 32],
                ..Default::default()
            };
            assert!(insert_one_nullifier(&pool, i, &new_nc, &new_on)
                .await
                .is_ok());
        }

        let batch_nc = get_batched_nullifier(&pool, start, end).await;
        assert!(batch_nc.is_ok());
        let batch_nc = batch_nc.unwrap();

        for (i, _nc) in (start..=end).zip(batch_nc) {
            let new_nc = [i as u8; 32];
            let new_on = OutgoingNote {
                ephemeral_public_key: [i as u8; 32],
                ..Default::default()
            };
            assert_eq!(_nc, (new_nc, new_on));
        }
    }

    #[tokio::test]
    async fn check_and_update_total_senders_receivers_should_work() {
        let tmp_db = tempfile::Builder::new()
            .prefix("tmp-utxo")
            .suffix(&".db")
            .tempfile()
            .unwrap();
        let db_url = tmp_db.path().to_str().unwrap();

        let pool = initialize_db_pool(db_url, 16).await;
        assert!(pool.is_ok());
        let pool = pool.unwrap();

        // insert total senders_receivers first
        let new_total = 100u128.to_le_bytes();
        assert!(update_or_insert_total_senders_receivers(&pool, &new_total)
            .await
            .is_ok());

        // check updated value
        let val = get_total_senders_receivers(&pool).await;
        assert!(val.is_ok());
        let val = val.unwrap();
        assert_eq!(val, new_total);

        // update the same total value
        assert!(update_or_insert_total_senders_receivers(&pool, &new_total)
            .await
            .is_ok());
        let val = get_total_senders_receivers(&pool).await;
        assert!(val.is_ok());
        let val = val.unwrap();
        assert_eq!(val, new_total);

        let new_total_1 = (u128::from_le_bytes(new_total) + 50).to_le_bytes();
        assert!(
            update_or_insert_total_senders_receivers(&pool, &new_total_1)
                .await
                .is_ok()
        );
        let val_1 = get_total_senders_receivers(&pool).await;
        assert!(val_1.is_ok());
        let val_1 = val_1.unwrap();
        assert_eq!(val_1, new_total_1);
    }

    #[tokio::test]
    async fn transaction_should_work() {
        // todo, ensure db transaction will work as expected
        assert!(true);
    }

    #[derive(serde::Serialize, codec::Encode)]
    struct NewPRV1 {
        rlen: u64,
        r: Vec<u8>,
        slen: u64,
        s: Vec<u8>,
    }

    #[derive(serde::Serialize)]
    struct NewPRV2 {
        rlen: u64,
        r: String,
        slen: u64,
        s: String,
    }

    #[test]
    fn test_the_optimize_of_pull_response_dense() -> Result<()> {
        let mut x = PullResponse {
            should_continue: false,
            receivers: vec![],
            senders: vec![],
            senders_receivers_total: 0u128.to_le_bytes(),
        };

        let size = 1024 * 8;
        for _ in 0..size {
            x.receivers.push((Default::default(), Default::default()));
            x.senders.push(Default::default());
        }
        let timer = std::time::Instant::now();
        let _d = serde_json::to_string(&x)?;
        println!("old time = {} ms", timer.elapsed().as_millis());

        let mut y = NewPRV1 {
            rlen: 0,
            r: Vec::new(),
            slen: 0,
            s: Vec::new(),
        };
        for _ in 0..size {
            y.rlen += 1;
            y.r.extend_from_slice(&[0u8; 132]);
            y.slen += 1;
            y.s.extend_from_slice(&[0u8; 32]);
        }
        {
            let timer = std::time::Instant::now();
            let _d = serde_json::to_string(&y)?;
            println!(
                "new v1 time = {} ms, len = {}, d = {:?}",
                timer.elapsed().as_millis(),
                _d.len(),
                &_d[0..100]
            );
        }

        let mut y = NewPRV2 {
            rlen: 0,
            r: String::new(),
            slen: 0,
            s: String::new(),
        };
        let mut r = Vec::new();
        let mut s = Vec::new();
        for _ in 0..size {
            y.rlen += 1;
            r.extend_from_slice(&[1u8; 132]);
            y.slen += 1;
            s.extend_from_slice(&[1u8; 32]);
        }
        y.r = base64::encode(r.as_slice());
        y.s = base64::encode(s.as_slice());
        {
            let timer = std::time::Instant::now();
            let _d = serde_json::to_string(&y)?;
            println!(
                "new v2 time = {} ms, len = {}, d = {:?}",
                timer.elapsed().as_millis(),
                _d.len(),
                &_d[0..100]
            );
        }

        {
            let timer = std::time::Instant::now();
            let _d = serde_json::to_string(&x.encode())?;
            println!(
                "new v3 time = {} ms, len = {}, d = {:?}",
                timer.elapsed().as_millis(),
                _d.len(),
                &_d[0..100]
            );
        }

        y.r = hex::encode(r.as_slice());
        y.s = hex::encode(s.as_slice());
        {
            let timer = std::time::Instant::now();
            let _d = serde_json::to_string(&y)?;
            println!(
                "new v4 time = {} ms, len = {}, d = {:?}",
                timer.elapsed().as_millis(),
                _d.len(),
                &_d[0..100]
            );
        }

        Ok(())
    }
}
