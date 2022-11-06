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

use crate::constants::MEGABYTE;
use crate::indexer::{MantaPayIndexerApiServer, MantaPayIndexerServer};
use crate::monitoring::IndexerMiddleware;
use crate::relayer::{
    relay_server::{MantaRelayApiServer, MantaRpcRelayServer},
    WsServerConfig,
};
use anyhow::{bail, Result};
use jsonrpsee::ws_server::{WsServerBuilder, WsServerHandle};
use std::os::unix::net::SocketAddr;

const FULL_NODE_BLOCK_GEN_INTERVAL_SEC: u8 = 12;

pub async fn start_service() -> Result<WsServerHandle> {
    let mut module = jsonrpsee::RpcModule::<()>::new(());

    let config = crate::utils::read_config()?;
    let full_node = config["indexer"]["configuration"]["full_node"]
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

    let port = config["indexer"]["configuration"]["port"]
        .as_integer()
        .ok_or(crate::IndexerError::WrongConfig)?;
    let monitor_port = config["indexer"]["configuration"]["prometheus_port"]
        .as_integer()
        .ok_or(crate::IndexerError::WrongConfig)?;
    let frequency = config["indexer"]["configuration"]["frequency"]
        .as_integer()
        .ok_or(crate::IndexerError::WrongConfig)?;
    if frequency >= FULL_NODE_BLOCK_GEN_INTERVAL_SEC as i64 {
        bail!(
            "frequency config({}) is larger than limit({})",
            frequency,
            FULL_NODE_BLOCK_GEN_INTERVAL_SEC
        );
    }

    // create relay rpc handler
    let relayer_rpc = MantaRpcRelayServer::new(full_node).await?;
    module.merge(relayer_rpc.into_rpc())?;

    let config = config["server"]["configuration"].to_string();
    let srv_config: WsServerConfig = toml::from_str(&config)?;

    let mut available_methods = module.method_names().collect::<Vec<_>>();
    available_methods.sort_unstable();
    module
        .register_method("indexer_rpc_methods", move |_, _| {
            Ok(serde_json::json!({
                "methods": available_methods,
            }))
        })
        .expect("infallible all other methods have their own address space; qed");

    let srv_addr = format!("127.0.0.1:{port}");
    let server = WsServerBuilder::new()
        .max_connections(srv_config.max_connections)
        .max_request_body_size(srv_config.max_request_body_size * MEGABYTE)
        .max_response_body_size(srv_config.max_response_body_size * MEGABYTE)
        .ping_interval(srv_config.ping_interval)
        .max_subscriptions_per_connection(srv_config.max_subscriptions_per_connection)
        .set_middleware(IndexerMiddleware::default())
        .build(srv_addr)
        .await?;

    let _addr = server.local_addr()?;
    let handle = server.start(module)?;

    let prometheus_addr = format!("127.0.0.1:{}", monitor_port);
    prometheus_exporter::start(prometheus_addr.parse()?)?;
    Ok(handle)
}
