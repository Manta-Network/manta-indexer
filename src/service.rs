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
use crate::indexer::{MAX_RECEIVERS, MAX_SENDERS};
use crate::relayer::{
    relay_server::{MantaRelayApiServer, MantaRpcRelayServer},
    WsServerConfig,
};
use anyhow::Result;
use jsonrpsee::ws_server::{WsServerBuilder, WsServerHandle};

pub async fn start_service() -> Result<WsServerHandle> {
    let mut module = jsonrpsee::RpcModule::<()>::new(());

    let config = crate::utils::read_config()?;
    let full_node = config["indexer"]["configuration"]["full_node"]
        .as_str()
        .ok_or(crate::IndexerError::WrongConfig)?;
    let rpc_method = config["indexer"]["configuration"]["rpc_method"]
        .as_str()
        .ok_or(crate::IndexerError::WrongConfig)?;
    let pool_size = config["db"]["configuration"]["pool_size"]
        .as_integer()
        .ok_or(crate::IndexerError::WrongConfig)? as u32;
    let db_path = config["db"]["configuration"]["db_path"]
        .as_str()
        .ok_or(crate::IndexerError::WrongConfig)?;

    let db_pool = crate::db::initialize_db_pool(db_path, pool_size).await?;

    // create indexer rpc handler
    let indexer_rpc = MantaPayIndexerServer::new(db_pool.clone());
    module.merge(indexer_rpc.into_rpc())?;

    let port = config["indexer"]["configuration"]["port"]
        .as_integer()
        .ok_or(crate::IndexerError::WrongConfig)?;
    let frequency = config["indexer"]["configuration"]["frequency"]
        .as_integer()
        .ok_or(crate::IndexerError::WrongConfig)? as u64;

    // create relay rpc handler
    let relayer_rpc = MantaRpcRelayServer::new(full_node).await?;
    module.merge(relayer_rpc.into_rpc())?;

    let content = crate::utils::read_config()?;
    let config = content["server"]["configuration"].to_string();
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
        // .set_middleware(IndexerMiddleware::default())
        .set_middleware(crate::logger::IndexerLogger)
        .build(srv_addr)
        .await?;

    let _addr = server.local_addr()?;
    let handle = server.start(module)?;

    // start syncing shards job
    let ws_client = crate::utils::create_ws_client(full_node).await?;
    // ensure the full node has this rpc method.
    crate::utils::is_the_rpc_methods_existed(&ws_client, rpc_method)
        .await
        .map_err(|_| crate::IndexerError::RpcMethodNotExists)?;

    crate::indexer::sync::start_sync_shards_job(
        &ws_client,
        &db_pool,
        (MAX_RECEIVERS, MAX_SENDERS),
        frequency,
    )
    .await?;

    Ok(handle)
}
