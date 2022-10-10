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

use anyhow::Result;
use rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::Mutex;

// Initialize sqlite as WAL mode(Write-Ahead Logging)
// https://sqlite.org/wal.html
pub async fn initialize_db(conn: Arc<Mutex<Connection>>) -> Result<()> {
    let _conn = conn.lock().await;
    _conn.execute("PRAGMA synchronous = OFF;", ())?;
    // Enable WAL mode
    _conn.execute("PRAGMA journal_mode = WAL;", ())?;

    Ok(())
}

pub async fn create_table(conn: Arc<Mutex<Connection>>, table: &str) -> Result<()> {
    conn.lock().await.execute(table, ())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_table_should_work() {
        let db_path = "/Users/jamie/my-repo/manta-indexer/sqlite-indexing.db";
        let table = "
            create table hacker (
                first_name text not null,
                last_name text not null,
                email text not null
            )
        ";
        let conn = crate::utils::open_db(db_path).await.unwrap();
        initialize_db(conn.clone()).await;
        create_table(conn, table).await;
        assert!(true);
    }
}
