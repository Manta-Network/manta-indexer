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

use crate::db::DbConfig;
use crate::logger::IndexerLogger;
use crate::types::PullResponse;
use anyhow::Result;
use jsonrpsee::ws_server::{WsServerBuilder, WsServerHandle};
use jsonrpsee::{
    core::{async_trait, RpcResult},
    proc_macros::rpc,
};
use manta_pay::signer::Checkpoint;
use sqlx::sqlite::SqlitePool;
use std::net::SocketAddr;

pub mod pull;
pub mod sync;

#[rpc(server, namespace = "mantaPay")]
pub trait MantaPayIndexerApi {
    #[method(name = "pullLedgerDiff")] // no blocking mode, we just query all shards from db
    async fn pull_ledger_diff(
        &self,
        checkpoint: Checkpoint,
        max_receivers: u64,
        max_senders: u64,
    ) -> RpcResult<PullResponse>;
}

pub struct MantaPayIndexerServer {
    pub db: SqlitePool, // db pool
    pub ws: String,     // the websocket url to local node
    pub db_config: DbConfig,
}

#[async_trait]
impl MantaPayIndexerApiServer for MantaPayIndexerServer {
    async fn pull_ledger_diff(
        &self,
        checkpoint: Checkpoint,
        max_receivers: u64,
        max_senders: u64,
    ) -> RpcResult<PullResponse> {
        let db = self.db.clone();

        let url = &self.ws;
        let client = crate::utils::create_ws_client(url).await?;
        // let response = sync::pull_ledger_diff(&db, &checkpoint, max_receivers, max_senders);

        // Ok(response)
        todo!();
    }
}

impl MantaPayIndexerServer {
    pub async fn start_server() -> Result<(SocketAddr, WsServerHandle)> {
        let server = WsServerBuilder::new()
            .set_middleware(IndexerLogger)
            .build("127.0.0.1:9800")
            .await?;

        let db_path = concat!(env!("CARGO_MANIFEST_DIR"), "/indexer.db");

        let db_config = DbConfig {
            pool_size: 16,
            db_url: db_path.to_owned(),
        };
        let db = crate::db::initialize_db_pool(&db_config.db_url, db_config.pool_size).await?;

        let ws = "127.0.0.1:9944".to_owned();
        let rpc = Self { db, ws, db_config };

        let addr = server.local_addr()?;
        let handle = server.start(rpc.into_rpc())?;
        Ok((addr, handle))
    }
}
