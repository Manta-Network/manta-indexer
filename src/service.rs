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
use crate::indexer::sync::INIT_SYNCING_FINISHED;
use crate::indexer::{MantaPayIndexerApiServer, MantaPayIndexerServer};
use crate::indexer::{MAX_RECEIVERS, MAX_SENDERS};
use crate::monitoring::IndexerMiddleware;
use crate::relayer::{
    relay_server::{MantaRelayApiServer, MantaRpcRelayServer},
    WsServerConfig,
};
use crate::types::RpcMethods;
use crate::utils::OnceStatic;
use anyhow::{bail, Result};
use frame_support::log::info;
use jsonrpsee::ws_server::{WsServerBuilder, WsServerHandle};
use std::sync::atomic::Ordering::SeqCst;
use std::time::Duration;

const FULL_NODE_BLOCK_GEN_INTERVAL_SEC: u8 = 12;
static RPC_METHODS: OnceStatic<RpcMethods> = OnceStatic::new("RPC_METHODS");

pub async fn start_service() -> Result<WsServerHandle> {
    let mut module = jsonrpsee::RpcModule::<()>::new(());

    let config = crate::utils::read_config()?;
    let full_node = config["indexer"]["configuration"]["full_node"]
        .as_str()
        .ok_or(crate::errors::IndexerError::WrongConfig)?;
    let rpc_method = config["indexer"]["configuration"]["rpc_method"]
        .as_str()
        .ok_or(crate::errors::IndexerError::WrongConfig)?;
    let pool_size = config["db"]["configuration"]["pool_size"]
        .as_integer()
        .ok_or(crate::errors::IndexerError::WrongConfig)? as u32;
    let db_path = config["db"]["configuration"]["db_path"]
        .as_str()
        .ok_or(crate::errors::IndexerError::WrongConfig)?;

    let db_pool = crate::db::initialize_db_pool(db_path, pool_size).await?;

    // create indexer rpc handler
    let indexer_rpc = MantaPayIndexerServer::new(db_pool.clone());
    module.merge(indexer_rpc.into_rpc())?;

    let port = config["indexer"]["configuration"]["port"]
        .as_integer()
        .ok_or(crate::errors::IndexerError::WrongConfig)?;
    let monitor_port = config["indexer"]["configuration"]["prometheus_port"]
        .as_integer()
        .ok_or(crate::errors::IndexerError::WrongConfig)?;
    let frequency = config["indexer"]["configuration"]["frequency"]
        .as_integer()
        .ok_or(crate::errors::IndexerError::WrongConfig)? as u64;
    if frequency >= FULL_NODE_BLOCK_GEN_INTERVAL_SEC as u64 {
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

    let mut available_methods = module
        .method_names()
        .map(|s| s.to_owned())
        .collect::<Vec<_>>();
    available_methods.sort_unstable();
    info!(target: "indexer", "all methods: {:?}", available_methods);
    RPC_METHODS.init(move || {
        Ok(RpcMethods {
            version: 1,
            methods: available_methods,
        })
    })?;

    // we may miss some methods, may add some methods that only exists in indexer.
    // so we rewrite the `rpc_methods` instead of relay full node's, since those two response is not quite same.
    // this way also allows polkadot-js found api only defined in indexer
    // see: https://polkadot.js.org/docs/api/start/rpc.custom#custom-definitions
    module
        .register_method("rpc_methods", move |_, _| Ok(RPC_METHODS.clone()))
        .expect("infallible all other methods have their own address space; qed");

    // start a backend syncing thread.
    {
        let ws_client = crate::utils::create_ws_client(full_node).await?;
        // ensure the full node has this rpc method.
        crate::utils::is_the_rpc_methods_existed(&ws_client, rpc_method)
            .await
            .map_err(|_| crate::errors::IndexerError::RpcMethodNotExists)?;
        crate::indexer::sync::start_sync_ledger_job(
            ws_client,
            db_pool,
            (MAX_RECEIVERS, MAX_SENDERS),
            frequency,
        );
        while !INIT_SYNCING_FINISHED.load(SeqCst) {
            info!(target: "indexer", "still wait for initialized syncing finished...");
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }

    // start monitoring service.
    {
        let prometheus_addr = format!("127.0.0.1:{}", monitor_port);
        prometheus_exporter::start(prometheus_addr.parse()?)?;
    }

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
    Ok(server.start(module)?)
}
