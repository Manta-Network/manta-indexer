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

use crate::logger::IndexerLogger;
use anyhow::Result;
use jsonrpsee::ws_server::{WsServerBuilder, WsServerHandle};
use jsonrpsee::{
    core::{async_trait, RpcResult},
    proc_macros::rpc,
};
use manta_pay::signer::{Checkpoint as _, RawCheckpoint};
use rusqlite::{Connection, Params};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

pub type PullResponse = Vec<u8>;
pub type Checkpoint = Vec<u8>;

#[rpc(server, namespace = "mantaPay")]
pub trait MantaPayIndexerApi {
    #[method(name = "pullLedgerDiff", blocking)]
    fn pull_ledger_diff(
        &self,
        checkpoint: Checkpoint,
        max_receivers: u64,
        max_senders: u64,
    ) -> RpcResult<PullResponse>;
}

pub struct MantaPayIndexerServer {
    pub db: Arc<Mutex<Connection>>,
}

#[async_trait]
impl MantaPayIndexerApiServer for MantaPayIndexerServer {
    fn pull_ledger_diff(
        &self,
        checkpoint: Checkpoint,
        max_receivers: u64,
        max_senders: u64,
    ) -> RpcResult<PullResponse> {
        // let db = self.db;
        // let query = format!("select k1, k2, utxo from ")
        // db.execute()

        Ok(b"Indexer server started!".to_vec())
    }
}

pub async fn start_server() -> Result<(SocketAddr, WsServerHandle)> {
    let server = WsServerBuilder::new()
        .set_middleware(IndexerLogger)
        .build("127.0.0.1:9800")
        .await?;

    let db_path = concat!(env!("CARGO_MANIFEST_DIR"), "/indexer.db");
    let db = crate::utils::open_db(db_path).await?;

    let rpc = MantaPayIndexerServer { db };

    let addr = server.local_addr()?;
    let handle = server.start(rpc.into_rpc())?;
    Ok((addr, handle))
}
