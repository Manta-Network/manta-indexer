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

    let one: Result<Shard, _> =
        sqlx::query_as("SELECT * FROM shards WHERE shard_index = ?1 and next_index = ?2;")
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

    let one = sqlx::query_as("SELECT * FROM shards WHERE shard_index = ?1 and next_index = ?2;")
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

    let batched_shard = sqlx::query_as(
        "SELECT * FROM shards WHERE shard_index = ?1 and next_index BETWEEN ?2 and ?3;",
    )
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

/// Instead of insert with specific idx, just append a new void number record with auto increasing idx primary key.
pub async fn append_void_number(pool: &SqlitePool, encoded_vn: Vec<u8>) -> Result<()> {
    let mut conn = pool.acquire().await?;
    sqlx::query("INSERT INTO void_number (vn) VALUES (?1)")
        .bind(encoded_vn)
        .execute(&mut conn)
        .await?;
    Ok(())
}

pub async fn get_len_of_void_number(pool: &SqlitePool) -> Result<usize> {
    Ok(sqlx::query(r#"SELECT count(*) as count FROM void_number;"#)
        .fetch_one(pool)
        .await?
        .get::<u32, _>("count") as usize)
}

pub async fn get_latest_check_point(pool: &SqlitePool) -> Result<Checkpoint> {
    let mut ckp = Checkpoint::default();

    let mut stream = tokio_stream::iter(0..=255u8);
    // If there's no any shard in db, return default checkpoint.
    while let Some(shard_index) = stream.next().await {
        let count_of_utxo: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM shards WHERE shard_index = ?;")
                .bind(shard_index)
                .fetch_one(pool)
                .await?;

        ckp.receiver_index[shard_index as usize] = count_of_utxo.0 as usize;
    }
    ckp.sender_index = get_len_of_void_number(pool).await?;

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
        .map_err(|_| crate::errors::IndexerError::DecodedError)?;

    Ok(one)
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
            .map_err(|_| crate::errors::IndexerError::DecodedError)?;
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
    if let Err(_) | Ok(0) = get_len_of_void_number(&pool).await {
        crate::indexer::sync::pull_all_shards_to_db(&pool, node).await?;
    }

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Shard;
    use rand::distributions::{Distribution, Uniform};

    #[tokio::test]
    async fn do_migration_should_work() {
        let pool = create_test_db_or_first_pull(true).await;
        assert!(pool.is_ok());
    }

    #[tokio::test]
    async fn get_one_shard_should_work() {
        let pool = create_test_db_or_first_pull(false).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        let shard_index_between = Uniform::from(0..=255); // [0, 256)
        let next_index_between = Uniform::from(0..300);
        let mut rng = rand::thread_rng();
        let shard_index = shard_index_between.sample(&mut rng);
        let next_index = next_index_between.sample(&mut rng);
        let one_shard = get_one_shard(&pool, shard_index, next_index).await;

        assert!(one_shard.is_ok());
        assert_eq!(one_shard.as_ref().unwrap().shard_index, shard_index);
    }

    #[tokio::test]
    async fn has_shard_should_work() {
        let pool = create_test_db_or_first_pull(false).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        let shard_index_between = Uniform::from(0..=255); // [0, 256)
        let next_index_between = Uniform::from(0..300);
        let mut rng = rand::thread_rng();
        let shard_index = shard_index_between.sample(&mut rng);
        let next_index = next_index_between.sample(&mut rng);
        assert!(has_shard(&pool, shard_index, next_index).await);

        let invalid_next_index = u64::MAX;

        // This shard should not exist.
        assert!(!has_shard(&pool, shard_index, invalid_next_index).await);
    }

    #[tokio::test]
    async fn get_batched_shards_should_work() {
        let pool = create_test_db_or_first_pull(false).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        let shard_index_between = Uniform::from(0..=255); // [0, 256)
        let next_index_between = Uniform::from(0..300);
        let mut rng = rand::thread_rng();
        let shard_index = shard_index_between.sample(&mut rng);
        let mut from_next_index = next_index_between.sample(&mut rng);
        let mut to_next_index = next_index_between.sample(&mut rng);

        if to_next_index < from_next_index {
            std::mem::swap(&mut to_next_index, &mut from_next_index);
        }
        assert!(to_next_index > from_next_index);

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
            .prefix("tmp-utxo")
            .suffix(&".db")
            .tempfile()
            .unwrap();
        let db_url = tmp_db.path().to_str().unwrap();

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
    async fn insert_void_numbers_should_work() {
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
            .prefix("tmp-utxo")
            .suffix(&".db")
            .tempfile()
            .unwrap();
        let db_url = tmp_db.path().to_str().unwrap();

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
