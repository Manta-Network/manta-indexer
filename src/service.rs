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

use crate::indexer::{MantaPayIndexerApiServer, MantaPayIndexerServer};
use crate::relayer::{
    middleware::IndexerMiddleware,
    relay_server::{MantaRelayApiServer, MantaRpcRelayServer},
    WsServerConfig,
};
use anyhow::Result;
use jsonrpsee::ws_server::{WsServerBuilder, WsServerHandle};

pub type RpcExtension = jsonrpsee::RpcModule<()>;

pub async fn start_service() -> Result<()> {
    let mut module = RpcExtension::new(());

    let config = crate::utils::read_config()?;
    let full_node = config["configuration"]["full_node"]
        .as_str()
        .ok_or(crate::IndexerError::WrongConfig)?;
    let pool_size = config["db"]["configuration"]["pool_size"]
        .as_integer()
        .ok_or(crate::IndexerError::WrongConfig)? as u32;
    let db_path = config["db"]["configuration"]["db_path"]
        .as_str()
        .ok_or(crate::IndexerError::WrongConfig)?;

    // create indexer rpc handler
    let indexer_rpc = MantaPayIndexerServer::new(db_path, pool_size, full_node).await?;
    module.merge(indexer_rpc.into_rpc())?;

    let port = config["configuration"]["port"]
        .as_str()
        .ok_or(crate::IndexerError::WrongConfig)?;
    let frequency = config["configuration"]["frequency"]
        .as_str()
        .ok_or(crate::IndexerError::WrongConfig)?;

    // create relay rpc handler
    let relayer_rpc = MantaRpcRelayServer::new(full_node).await?;
    module.merge(relayer_rpc.into_rpc())?;

    let content = crate::utils::read_config()?;
    let config = content["server"]["configuration"].to_string();
    let srv_config: WsServerConfig = toml::from_str(&config)?;
    let client = crate::utils::create_ws_client(full_node).await?;

    let srv_addr = format!("127.0.0.1:{port}");
    let server = WsServerBuilder::new()
        .max_connections(srv_config.max_connections)
        .max_request_body_size(srv_config.max_request_body_size)
        .max_response_body_size(srv_config.max_response_body_size)
        .ping_interval(srv_config.ping_interval)
        .max_subscriptions_per_connection(srv_config.max_subscriptions_per_connection)
        .set_middleware(IndexerMiddleware::default())
        .build(srv_addr)
        .await?;

    let addr = server.local_addr()?;
    let handle = server.start(module)?;
    Ok(())
}
