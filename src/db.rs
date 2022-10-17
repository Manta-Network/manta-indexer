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
    migrate::Migrator,
    sqlite::{SqlitePool, SqlitePoolOptions},
    Error as SqlxError,
};
use std::{env, path::Path};
use tokio_stream::StreamExt;

/// Initialize sqlite as WAL mode(Write-Ahead Logging)
// https://sqlite.org/wal.html
pub async fn initialize_db_pool() -> Result<SqlitePool> {
    let m = Migrator::new(Path::new("./migrations")).await?;

    let db_url = env::var("DATABASE_URL")?;
    // assert!(m.database_exists(&db_url).await?);

    // debug or test build will this this db url.
    // #[cfg(any(test, build_type = "debug"))]
    // let db_url = env::var("DATABASE_TEST_URL")?;

    // todo, set a proper pool size
    let pool_size = 16u32;
    let pool = SqlitePoolOptions::new()
        .max_lifetime(None)
        .idle_timeout(None)
        .max_connections(pool_size)
        .connect(&db_url)
        .await?;
    m.run(&pool).await?;

    sqlx::query!(r#"PRAGMA synchronous = OFF;"#)
        .execute(&pool)
        .await?;

    // Enable WAL mode
    sqlx::query!(r#"PRAGMA synchronous = OFF;"#)
        .execute(&pool)
        .await?;

    Ok(pool)
}

#[cfg(test)]
pub async fn initialize_db_pool_for_test(db_url: &str) -> Result<SqlitePool> {
    // let m = Migrator::new(Path::new("./migrations")).await?;

    // todo, set a proper pool size
    let pool = SqlitePoolOptions::new()
        .max_lifetime(None)
        .idle_timeout(None)
        .max_connections(16)
        .connect(&db_url)
        .await?;
    // let mut conn = pool.acquire().await?;
    // m.run(&mut conn).await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    sqlx::query!(r#"PRAGMA synchronous = OFF;"#)
        .execute(&pool)
        .await?;

    // Enable WAL mode
    sqlx::query!(r#"PRAGMA synchronous = OFF;"#)
        .execute(&pool)
        .await?;

    Ok(pool)
}

/// Whether the shard exists in the db or not.
pub async fn has_shard(pool: &SqlitePool, shard_index: u8, next_index: u64) -> bool {
    let n = next_index as i64;
    let one = sqlx::query!(
        "SELECT shard_index, next_index FROM shards WHERE shard_index = ? and next_index = ?",
        shard_index,
        n
    )
    .fetch_one(pool)
    .await;

    match one {
        Ok(_) => true,
        Err(SqlxError::RowNotFound) => false,
        Err(_) => false, // this case should happen, in theory
    }
}

/// Get a single shard from db.
pub async fn get_a_single_shard(
    pool: &SqlitePool,
    shard_index: u8,
    next_index: u64,
) -> Result<Shard> {
    let n = next_index as i64;
    let one_shard = sqlx::query_as_unchecked!(
        Shard,
        "SELECT shard_index, next_index, utxo FROM shards WHERE shard_index = ? and next_index = ?",
        shard_index,
        n
    )
    .fetch_one(pool)
    .await;

    one_shard.map_err(From::from)
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
    let batched_shard = sqlx::query_as_unchecked!(
        Shard,
        "SELECT shard_index, next_index, utxo FROM shards WHERE shard_index = ? and next_index BETWEEN ? and ?",
        shard_index,
        from, to
    )
    .fetch_all(pool)
    .await;

    batched_shard.map_err(From::from)
}

pub async fn insert_a_single_shard(
    pool: &SqlitePool,
    shard_index: u8,
    next_index: u64,
    encoded_utxo: Vec<u8>,
) -> Result<()> {
    let n = next_index as i64;

    let mut conn = pool.acquire().await?;
    let row_at = sqlx::query!(
        "INSERT INTO shards (shard_index, next_index, utxo) VALUES (?, ?, ?)",
        shard_index,
        n,
        encoded_utxo
    )
    .execute(&mut conn)
    .await?;

    Ok(())
}

pub async fn insert_a_single_void_number(
    pool: &SqlitePool,
    vn_index: u64,
    encoded_vn: Vec<u8>,
) -> Result<()> {
    todo!();
    // let n = vn_index as i64;

    // let mut conn = pool.acquire().await?;
    // let row_at = sqlx::query!(
    //     "INSERT INTO void_number (idx, vn) VALUES (?, ?)",
    //     n,
    //     encoded_vn
    // )
    // .execute(&mut conn)
    // .await;

    // Ok(())
}

pub async fn get_len_of_void_number(pool: &SqlitePool) -> Result<usize> {
    todo!();
    // let vns = sqlx::query!(
    //     r#"SELECT vn FROM void_number;"#
    // )
    // .execute(&mut conn)
    // .await;

    // vns.map(|v| v.len())
}

pub async fn get_latest_check_point(pool: &SqlitePool) -> Result<Checkpoint> {
    let mut ckp = Checkpoint::default();

    let mut stream = tokio_stream::iter(0..=255u8);
    // If there's no any shard in db, return default checkpoint.
    while let Some(shard_index) = stream.next().await {
        let batched_shard = sqlx::query!(
            "SELECT shard_index FROM shards WHERE shard_index = ?;",
            shard_index,
        )
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
    let one = sqlx::query!("SELECT vn FROM void_number WHERE idx = ?;", n)
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
    let one = sqlx::query_as_unchecked!(VoidNumber, "SELECT vn FROM void_number WHERE idx = ?;", n)
        .fetch_one(pool)
        .await;

    one.map_err(From::from)
}

pub async fn get_batched_void_number(
    pool: &SqlitePool,
    from_vn_index: u64,
    to_vn_index: u64,
) -> Result<SenderChunk> {
    let from = from_vn_index as i64;
    let to = to_vn_index as i64;
    let batched_vns = sqlx::query_as_unchecked!(
        VoidNumber,
        "SELECT vn FROM void_number WHERE idx BETWEEN ? and ?;",
        from,
        to
    )
    .fetch_all(pool)
    .await;

    batched_vns.map_err(From::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Shard;
    use sqlx::migrate::MigrateDatabase;

    #[tokio::test]
    async fn do_migration_should_work() {
        dotenvy::dotenv().ok();
        let pool = initialize_db_pool().await;
        assert!(pool.is_ok());
    }

    #[tokio::test]
    async fn get_a_single_shard_should_work() {
        dotenvy::dotenv().ok();
        let pool = initialize_db_pool().await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        let shard_index = 200u8;
        let next_index = 10u64;
        let one_shard = get_a_single_shard(&pool, shard_index, next_index).await;

        assert!(one_shard.is_ok());
        assert_eq!(one_shard.as_ref().unwrap().shard_index, shard_index);
    }

    #[tokio::test]
    async fn has_shard_should_work() {
        dotenvy::dotenv().ok();
        let pool = initialize_db_pool().await;
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
    async fn get_batched_shards_should_work() {
        dotenvy::dotenv().ok();
        let pool = initialize_db_pool().await;
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
    async fn insert_a_single_shard_should_work() {
        // let tmp_db = tempfile::Builder::new()
        //     .prefix("sqlite:dolphin-tmp-shards")
        //     .suffix(&".db")
        //     .tempfile()
        //     .unwrap();
        // let db_url = tmp_db.path().to_str().unwrap();
        let db_url = "dolphin-tmp-shards.db";
        if let Ok(true) = sqlx::sqlite::Sqlite::database_exists(&db_url).await {
            ();
        } else {
            assert!(sqlx::sqlite::Sqlite::create_database(db_url).await.is_ok());
        }

        let pool = initialize_db_pool_for_test(db_url).await;
        let pool = pool.unwrap();
        // assert!(pool.is_ok());

        let utxo = vec![1u8, 132];
        let shard_index = 0u8;
        let next_index = 0u64;
        assert!(
            insert_a_single_void_number(&pool, shard_index, next_index, utxo)
                .await
                .is_ok()
        );

        let shard = get_a_single_shard(&pool, shard_index, next_index).await;
        // assert!(shard.is_ok());

        dbg!(&shard);
    }

    #[tokio::test]
    async fn get_latest_check_point_should_work() {
        dotenvy::dotenv().ok();
        let pool = initialize_db_pool().await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        let ckp = get_latest_check_point(&pool).await;
        assert_eq!(ckp.as_ref().unwrap().sender_index, 102612);
    }
}
