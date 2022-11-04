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

use crate::types::PullResponse;
use anyhow::Result;
use jsonrpsee::{
    core::{async_trait, error::Error as JsonRpseeError, RpcResult},
    proc_macros::rpc,
    ws_client::WsClient,
};
use manta_pay::signer::Checkpoint;
use sqlx::sqlite::SqlitePool;
use std::sync::Arc;

pub mod pull;
pub mod sync;

#[rpc(server, namespace = "mantaPay")]
pub trait MantaPayIndexerApi {
    #[method(name = "pull_ledger_diff")] // no blocking mode, we just query all shards from db
    async fn pull_ledger_diff(
        &self,
        checkpoint: Checkpoint,
        max_receivers: u64,
        max_senders: u64,
    ) -> RpcResult<PullResponse>;
}

pub struct MantaPayIndexerServer {
    // db pool
    pub db_pool: SqlitePool,
    // the websocket client connects to local full node
    pub full_node: Arc<WsClient>,
}

#[async_trait]
impl MantaPayIndexerApiServer for MantaPayIndexerServer {
    async fn pull_ledger_diff(
        &self,
        checkpoint: Checkpoint,
        max_receivers: u64,
        max_senders: u64,
    ) -> RpcResult<PullResponse> {
        let response =
            pull::pull_ledger_diff(&self.db_pool, &checkpoint, max_receivers, max_senders)
                .await
                .map_err(|_| JsonRpseeError::AlreadyStopped)?;

        Ok(response)
    }
}

impl MantaPayIndexerServer {
    pub async fn new(db_url: &str, pool_size: u32, full_node: &str) -> Result<Self> {
        let db_pool = crate::db::initialize_db_pool(db_url, pool_size).await?;
        let full_node = crate::utils::create_ws_client(full_node).await?;

        Ok(MantaPayIndexerServer {
            db_pool,
            full_node: Arc::new(full_node),
        })
    }
}
