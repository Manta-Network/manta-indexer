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

use crate::types::{SenderChunk, Shard, VoidNumber};
use anyhow::Result;
use manta_pay::signer::Checkpoint;
use sqlx::{
    migrate::{MigrateDatabase, Migrator},
    sqlite::{SqlitePool, SqlitePoolOptions},
    Acquire, Error as SqlxError, Row,
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

/// Whether the shard exists in the db or not.
pub async fn has_shard(pool: &SqlitePool, shard_index: u8, next_index: u64) -> bool {
    let n = next_index as i64;

    let one = sqlx::query("SELECT shard_index, next_index, utxo FROM shards WHERE shard_index = ?1 and next_index = ?2;")
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
pub async fn get_one_shard(pool: &SqlitePool, shard_index: u8, next_index: u64) -> Result<Shard> {
    let n = next_index as i64;

    let one = sqlx::query_as("SELECT shard_index, next_index, utxo FROM shards WHERE shard_index = ?1 and next_index = ?2;")
        .bind(shard_index)
        .bind(n)
        .fetch_one(pool)
        .await;

    one.map_err(From::from)
}

/// Get a batched shard from db.
/// [from_next_index, to_next_index], including the start and the end.
pub async fn get_batched_shards(
    pool: &SqlitePool,
    shard_index: u8,
    from_next_index: u64,
    to_next_index: u64,
) -> Result<Vec<Shard>> {
    let from = from_next_index as i64;
    let to = to_next_index as i64;

    let batched_shard = sqlx::query_as("SELECT shard_index, next_index, utxo FROM shards WHERE shard_index = ?1 and next_index BETWEEN ?2 and ?3;")
        .bind(shard_index)
        .bind(from)
        .bind(to)
        .fetch_all(pool)
        .await;

    batched_shard.map_err(From::from)
}

pub async fn insert_one_shard(
    pool: &SqlitePool,
    shard_index: u8,
    next_index: u64,
    encoded_utxo: Vec<u8>,
) -> Result<()> {
    let n = next_index as i64;

    let mut conn = pool.acquire().await?;
    let mut tx = conn.begin().await?;
    let _row_at =
        sqlx::query("INSERT INTO shards (shard_index, next_index, utxo) VALUES (?1, ?2, ?3);")
            .bind(shard_index)
            .bind(n)
            .bind(encoded_utxo)
            .execute(&mut tx)
            .await?;

    tx.commit().await?;

    Ok(())
}

pub async fn insert_one_void_number(
    pool: &SqlitePool,
    vn_index: u64,
    encoded_vn: Vec<u8>,
) -> Result<()> {
    let n = vn_index as i64;

    let mut conn = pool.acquire().await?;
    let _row_at = sqlx::query("INSERT INTO void_number (idx, vn) VALUES (?1, ?2)")
        .bind(n)
        .bind(encoded_vn)
        .execute(&mut conn)
        .await;

    Ok(())
}

pub async fn get_len_of_void_number(pool: &SqlitePool) -> Result<usize> {
    let vns = sqlx::query(r#"SELECT vn FROM void_number;"#)
        .fetch_all(pool)
        .await?;

    Ok(vns.len())
}

pub async fn get_latest_check_point(pool: &SqlitePool) -> Result<Checkpoint> {
    let mut ckp = Checkpoint::default();

    let mut stream = tokio_stream::iter(0..=255u8);
    // If there's no any shard in db, return default checkpoint.
    while let Some(shard_index) = stream.next().await {
        let batched_shard: Vec<Shard> =
            sqlx::query_as("SELECT * FROM shards WHERE shard_index = ?;")
                .bind(shard_index)
                .fetch_all(pool)
                .await?;

        ckp.receiver_index[shard_index as usize] = batched_shard.len();
    }
    ckp.sender_index = ckp.receiver_index.iter().sum();

    Ok(ckp)
}

/// Whether the void number exists in the db or not.
pub async fn has_void_number(pool: &SqlitePool, vn_index: u64) -> bool {
    let n = vn_index as i64;
    let one = sqlx::query("SELECT vn FROM void_number WHERE idx = ?1;")
        .bind(n)
        .fetch_one(pool)
        .await;

    match one {
        Ok(_) => true,
        Err(SqlxError::RowNotFound) => false,
        Err(_) => false, // this case should happen, in theory
    }
}

pub async fn get_one_void_number(pool: &SqlitePool, vn_index: u64) -> Result<VoidNumber> {
    let n = vn_index as i64;
    let one_row = sqlx::query("SELECT vn FROM void_number WHERE idx = ?1;")
        .bind(n)
        .fetch_one(pool)
        .await?;
    let one: VoidNumber = one_row
        .get::<Vec<u8>, _>("vn")
        .try_into()
        .map_err(|_| crate::IndexerError::DecodedError)?;

    Ok(one)
}

pub async fn get_length_of_void_number(pool: &SqlitePool) -> Result<usize> {
    let length = sqlx::query("SELECT vn FROM void_number;")
        .fetch_all(pool)
        .await?
        .len();
    Ok(length)
}

pub async fn get_batched_void_number(
    pool: &SqlitePool,
    from_vn_index: u64,
    to_vn_index: u64,
) -> Result<SenderChunk> {
    let from = from_vn_index as i64;
    let to = to_vn_index as i64;
    let batched_vns = sqlx::query("SELECT vn FROM void_number WHERE idx BETWEEN ?1 and ?2;")
        .bind(from)
        .bind(to)
        .fetch_all(pool)
        .await?;
    let mut vns = Vec::with_capacity(batched_vns.len());
    for i in &batched_vns {
        let vn: VoidNumber = i
            .get::<Vec<u8>, _>("vn")
            .try_into()
            .map_err(|_| crate::IndexerError::DecodedError)?;
        vns.push(vn)
    }

    Ok(vns)
}

pub async fn update_or_insert_total_senders_receivers(
    pool: &SqlitePool,
    new_total: i64,
) -> Result<()> {
    if let Ok(_val) = get_total_senders_receivers(pool).await {
        // the row exist, update it
        if _val != new_total {
            let mut conn = pool.acquire().await?;
            let mut tx = conn.begin().await?;
            let _ = sqlx::query("UPDATE senders_receivers_total SET total = ?1;")
                .bind(new_total)
                .execute(&mut tx)
                .await?;
            tx.commit().await?;
        }
    } else {
        let mut conn = pool.acquire().await?;
        let mut tx = conn.begin().await?;
        let _ = sqlx::query("INSERT INTO senders_receivers_total (total) VALUES (?1);")
            .bind(new_total)
            .execute(&mut tx)
            .await?;

        tx.commit().await?;
    }

    Ok(())
}

pub async fn get_total_senders_receivers(pool: &SqlitePool) -> Result<i64> {
    let total: (i64,) = sqlx::query_as("SELECT total FROM senders_receivers_total;")
        .fetch_one(pool)
        .await?;

    Ok(total.0)
}

#[cfg(test)]
async fn clean_up(conn: &mut SqlitePool, table_name: &str) -> Result<()> {
    use sqlx::Executor;

    let going_to_drop = format!("DROP TABLE {}", table_name);
    conn.execute(&*going_to_drop).await.ok();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Shard;
    use sqlx::migrate::MigrateDatabase;

    #[tokio::test]
    #[ignore = "todo, use Georgi's stress test to generate related utxos"]
    async fn do_migration_should_work() {
        let db_path = "dolphin-shards.db";
        let pool_size = 16u32;
        let pool = initialize_db_pool(db_path, pool_size).await;
        assert!(pool.is_ok());
    }

    #[tokio::test]
    #[ignore = "todo, use Georgi's stress test to generate related utxos"]
    async fn get_one_shard_should_work() {
        let db_path = "dolphin-shards.db";
        let pool_size = 16u32;
        let pool = initialize_db_pool(db_path, pool_size).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        let shard_index = 200u8;
        let next_index = 10u64;
        let one_shard = get_one_shard(&pool, shard_index, next_index).await;

        assert!(one_shard.is_ok());
        dbg!(&one_shard);
        assert_eq!(one_shard.as_ref().unwrap().shard_index, shard_index);
    }

    #[tokio::test]
    #[ignore = "todo, use Georgi's stress test to generate related utxos"]
    async fn has_shard_should_work() {
        let db_path = "dolphin-shards.db";
        let pool_size = 16u32;
        let pool = initialize_db_pool(db_path, pool_size).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        let shard_index = 200u8;
        let next_index = 10u64;
        assert!(has_shard(&pool, shard_index, next_index).await);

        let invalid_next_index = 10_000_000_000u64;

        // This shard should not exist.
        assert!(!has_shard(&pool, shard_index, invalid_next_index).await);
    }

    #[tokio::test]
    #[ignore = "todo, use Georgi's stress test to generate related utxos"]
    async fn get_batched_shards_should_work() {
        let db_path = "dolphin-shards.db";
        let pool_size = 16u32;
        let pool = initialize_db_pool(db_path, pool_size).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        let shard_index = 200u8;
        let (from_next_index, to_next_index) = (10u64, 30u64);
        let batched_shards =
            get_batched_shards(&pool, shard_index, from_next_index, to_next_index).await;

        assert!(batched_shards.is_ok());

        let batched_shards = batched_shards.unwrap();
        assert_eq!(
            batched_shards.len(),
            (to_next_index - from_next_index + 1) as usize
        );
    }

    #[tokio::test]
    async fn insert_one_shard_should_work() {
        let tmp_db = tempfile::Builder::new()
            .prefix("tmp-shards")
            .suffix(&".db")
            .tempfile()
            .unwrap();
        let db_url = tmp_db.path().to_str().unwrap();
        if let Ok(true) = sqlx::sqlite::Sqlite::database_exists(&db_url).await {
            ();
        } else {
            sqlx::sqlite::CREATE_DB_WAL.store(true, std::sync::atomic::Ordering::Release);
            assert!(sqlx::sqlite::Sqlite::create_database(db_url).await.is_ok());
        }

        let pool = initialize_db_pool(db_url, 16).await;
        assert!(pool.is_ok());
        let mut pool = pool.unwrap();

        let utxo = vec![100u8; 132];
        let shard_index = 0u8;
        let next_index = 0u64;
        assert!(
            insert_one_shard(&pool, shard_index, next_index, utxo.clone())
                .await
                .is_ok()
        );

        let shard = get_one_shard(&pool, shard_index, next_index).await;
        assert!(shard.is_ok());
        let shard = shard.unwrap();

        let orignal_shard = Shard {
            shard_index,
            next_index: next_index as i64,
            utxo: utxo,
        };

        assert!(clean_up(&mut pool, "shards").await.is_ok());
        assert_eq!(orignal_shard, shard);
    }

    #[tokio::test]
    #[ignore = "todo, use Georgi's stress test to generate related utxos"]
    async fn get_latest_check_point_should_work() {
        let db_path = "dolphin-shards.db";
        let pool_size = 16u32;
        let pool = initialize_db_pool(db_path, pool_size).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        let ckp = get_latest_check_point(&pool).await;
        assert_eq!(ckp.as_ref().unwrap().sender_index, 102612);
    }

    #[tokio::test]
    async fn insert_void_numbers_should_work() {
        let tmp_db = tempfile::Builder::new()
            .prefix("tmp-shards")
            .suffix(&".db")
            .tempfile()
            .unwrap();
        let db_url = tmp_db.path().to_str().unwrap();
        if let Ok(true) = sqlx::sqlite::Sqlite::database_exists(&db_url).await {
            ();
        } else {
            sqlx::sqlite::CREATE_DB_WAL.store(true, std::sync::atomic::Ordering::Release);
            assert!(sqlx::sqlite::Sqlite::create_database(db_url).await.is_ok());
        }

        let pool = initialize_db_pool(db_url, 16).await;
        assert!(pool.is_ok());
        let pool = pool.unwrap();

        let i = 1;
        let vn = [1u8; 32];
        assert!(insert_one_void_number(&pool, i, vn.clone().into())
            .await
            .is_ok());

        // query void number
        let _vn = get_one_void_number(&pool, i).await;
        assert_eq!(_vn.unwrap(), vn);

        let (start, end) = (1u64, 5u64);
        for i in start..=end {
            let vn = [i as u8; 32];
            assert!(insert_one_void_number(&pool, i, vn.clone().into())
                .await
                .is_ok());
        }

        let batch_vn = get_batched_void_number(&pool, start, end).await;
        assert!(batch_vn.is_ok());
        let batch_vn = batch_vn.unwrap();

        for (i, _vn) in (start..=end).zip(batch_vn) {
            let vn = [i as u8; 32];
            assert_eq!(_vn, vn);
        }
    }

    #[tokio::test]
    async fn check_and_update_total_senders_receivers_should_work() {
        let tmp_db = tempfile::Builder::new()
            .prefix("tmp-shards")
            .suffix(&".db")
            .tempfile()
            .unwrap();
        let db_url = tmp_db.path().to_str().unwrap();
        if let Ok(true) = sqlx::sqlite::Sqlite::database_exists(&db_url).await {
            ();
        } else {
            sqlx::sqlite::CREATE_DB_WAL.store(true, std::sync::atomic::Ordering::Release);
            assert!(sqlx::sqlite::Sqlite::create_database(db_url).await.is_ok());
        }

        let pool = initialize_db_pool(db_url, 16).await;
        assert!(pool.is_ok());
        let pool = pool.unwrap();

        // insert total senders_receivers first
        let new_total = 100;
        assert!(update_or_insert_total_senders_receivers(&pool, new_total)
            .await
            .is_ok());

        // check updated value
        let val = get_total_senders_receivers(&pool).await;
        assert!(val.is_ok());
        let val = val.unwrap();
        assert_eq!(val, new_total);

        // update the same total value
        assert!(update_or_insert_total_senders_receivers(&pool, new_total)
            .await
            .is_ok());
        let val = get_total_senders_receivers(&pool).await;
        assert!(val.is_ok());
        let val = val.unwrap();
        assert_eq!(val, new_total);

        let new_total_1 = new_total + 50;
        assert!(update_or_insert_total_senders_receivers(&pool, new_total_1)
            .await
            .is_ok());
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
}
