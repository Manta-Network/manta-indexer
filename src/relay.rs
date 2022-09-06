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

use crate::logger::RelayerLogger;
use anyhow::Result;
use jsonrpsee::ws_server::{WsServerBuilder, WsServerHandle};
use jsonrpsee::{
    core::{async_trait, RpcResult},
    proc_macros::rpc,
};
use manta_pay::signer::{Checkpoint as _, RawCheckpoint};
use rusqlite::{Connection, Params};
use sp_core::Bytes;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

pub type Hash = sp_core::H256;

pub struct MantaRpcRelayServer;

#[rpc(server)]
pub trait MantaRelayApi {
    #[method(name = "state_getMetadata", blocking)]
    fn metadata(&self) -> RpcResult<Bytes>;

    // #[method(name = "chain_getBlockHash", aliases = ["chain_getHead"], blocking)]
    // fn block_hash(
    // 	&self,
    // 	hash: Option<ListOrValue<NumberOrHex>>,
    // ) -> RpcResult<ListOrValue<Option<Hash>>>;
}

#[async_trait]
impl MantaRelayApiServer for MantaRpcRelayServer {
    fn metadata(&self) -> RpcResult<Bytes> {
        Ok(Bytes(b"Indexer server started!".to_vec()))
    }
}

pub async fn start_relayer_server() -> Result<(SocketAddr, WsServerHandle)> {
    let server = WsServerBuilder::new()
        .set_middleware(RelayerLogger)
        .build("127.0.0.1:9988")
        .await?;

    let addr = server.local_addr()?;
    let handle = server.start(MantaRpcRelayServer.into_rpc())?;
    Ok((addr, handle))
}
