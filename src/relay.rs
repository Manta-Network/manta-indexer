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
use crate::types::{Health, RpcMethods};
use anyhow::Result;
use jsonrpsee::ws_server::{WsServerBuilder, WsServerHandle};
use jsonrpsee::{
    core::{async_trait, RpcResult},
    proc_macros::rpc, SubscriptionSink,
    types::{EmptyParams, SubscriptionResult}, rpc_params,
};
use jsonrpsee::core::client::{ClientT, Subscription, SubscriptionClientT};
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use sp_rpc::{list::ListOrValue, number::NumberOrHex};
use manta_pay::signer::{Checkpoint as _, RawCheckpoint};
use rusqlite::{Connection, Params};
use sp_core::Bytes;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use sp_core::storage::{StorageChangeSet, StorageData, StorageKey};
use sp_runtime::traits::BlakeTwo256;
use std::time::Duration;

pub type Hash = sp_core::H256;

/// Arbitrary properties defined in chain spec as a JSON object
pub type Properties = serde_json::map::Map<String, serde_json::Value>;

/// An index to a block.
pub type BlockNumber = u32;

/// Block header type as expected by this runtime.
pub type Header = sp_runtime::generic::Header<BlockNumber, BlakeTwo256>;

pub struct MantaRpcRelayServer {
    pub client: WsClient,
}

#[rpc(client, server)]
pub trait MantaRelayApi {
    #[method(name = "state_getMetadata")]
    async fn metadata(&self) -> RpcResult<Bytes>;

    #[method(name = "chain_getBlockHash", aliases = ["chain_getHead"])]
    async fn block_hash(
    	&self,
    	hash: Option<ListOrValue<NumberOrHex>>,
    ) -> RpcResult<ListOrValue<Option<Hash>>>;

    #[method(name = "state_getRuntimeVersion", aliases = ["chain_getRuntimeVersion"])]
	async fn runtime_version(&self, hash: Option<Hash>) -> RpcResult<sp_version::RuntimeVersion>;

    #[method(name = "system_chain")]
	async fn system_chain(&self) -> RpcResult<String>;

    #[method(name = "system_properties")]
	async fn system_properties(&self) -> RpcResult<Properties>;

    #[method(name = "rpc_methods")]
	async fn rpc_methods(&self) -> RpcResult<RpcMethods>;

    #[subscription(
		name = "state_subscribeRuntimeVersion" => "state_runtimeVersion",
		unsubscribe = "state_unsubscribeRuntimeVersion",
		aliases = ["chain_subscribeRuntimeVersion"],
		unsubscribe_aliases = ["chain_unsubscribeRuntimeVersion"],
		item = sp_version::RuntimeVersion,
	)]
	fn subscribe_runtime_version(&self);

    #[method(name = "system_health")]
	async fn system_health(&self) -> RpcResult<Health>;

    #[method(name = "state_queryStorageAt")]
	async fn query_storage_at(
		&self,
		keys: Vec<StorageKey>,
		at: Option<Hash>,
	) -> RpcResult<Vec<StorageChangeSet<Hash>>>;

    #[method(name = "chain_getHeader")]
	async fn header(&self, hash: Option<Hash>) -> RpcResult<Option<Header>>;

    #[method(name = "author_submitExtrinsic")]
	async fn submit_extrinsic(&self, extrinsic: Bytes) -> RpcResult<Hash>;

    #[method(name = "chain_getFinalizedHead", aliases = ["chain_getFinalisedHead"])]
	async fn finalized_head(&self) -> RpcResult<Hash>;

    #[subscription(
		name = "state_subscribeStorage" => "state_storage",
		unsubscribe = "state_unsubscribeStorage",
		item = StorageChangeSet<Hash>,
	)]
	fn subscribe_storage(&self, keys: Option<Vec<StorageKey>>);
}

#[async_trait]
impl MantaRelayApiServer for MantaRpcRelayServer {
    async fn metadata(&self) -> RpcResult<Bytes> {
        let metadata = self.client.request::<Bytes>("state_getMetadata", None).await?;
        Ok(metadata)
    }

    async fn block_hash(
    	&self,
    	hash: Option<ListOrValue<NumberOrHex>>,
    ) -> RpcResult<ListOrValue<Option<Hash>>> {
        let block = self.client.request::<ListOrValue<Option<Hash>>>("chain_getBlockHash", None).await?;

        Ok(block)
    }

    async fn runtime_version(&self, hash: Option<Hash>) -> RpcResult<sp_version::RuntimeVersion> {
        let rt_version = self.client.request::<sp_version::RuntimeVersion>("state_getRuntimeVersion", None).await?;

        Ok(rt_version)
    }

	async fn system_chain(&self) -> RpcResult<String> {
        let chain = self.client.request::<String>("system_chain", None).await?;

        Ok(chain)
    }

    async fn system_properties(&self) -> RpcResult<Properties> {
        let properties = self.client.request::<Properties>("system_properties", None).await?;

        Ok(properties)
    }

    async fn rpc_methods(&self) -> RpcResult<RpcMethods> {
        let methods = self.client.request::<RpcMethods>("rpc_methods", None).await?;

        Ok(methods)
    }

    // Subscription methods must not be `async`
    fn subscribe_runtime_version(&self, mut sink: SubscriptionSink) -> SubscriptionResult {
        let id = futures::executor::block_on(async {
            let id: Result<Subscription<i32>, _> = self.client.subscribe("state_subscribeRuntimeVersion", None, "state_unsubscribeRuntimeVersion").await;
        });

        sink.send(&());

        Ok(())
    }

    async fn system_health(&self) -> RpcResult<Health> {
        let health = self.client.request::<Health>("system_health", None).await?;

        Ok(health)
    }

    async fn query_storage_at(
		&self,
		keys: Vec<StorageKey>,
		at: Option<Hash>,
	) -> RpcResult<Vec<StorageChangeSet<Hash>>> {
        let param = rpc_params![keys];
        let storage = self.client.request::<Vec<StorageChangeSet<Hash>>>("state_queryStorageAt", param).await?;

        Ok(storage)
    }

    async fn header(&self, hash: Option<Hash>) -> RpcResult<Option<Header>> {
        let header = self.client.request::<Option<Header>>("state_queryStorageAt", rpc_params![hash]).await?;

        Ok(header)
    }

	async fn submit_extrinsic(&self, extrinsic: Bytes) -> RpcResult<Hash> {
        let hash = self.client.request::<Hash>("author_submitExtrinsic", rpc_params![extrinsic]).await?;

        Ok(hash)
    }

    async fn finalized_head(&self) -> RpcResult<Hash> {
        let hash = self.client.request::<Hash>("chain_getFinalizedHead", None).await?;

        Ok(hash)
    }

    fn subscribe_storage(&self, mut sink: SubscriptionSink, keys: Option<Vec<StorageKey>>) -> SubscriptionResult {
        let id = futures::executor::block_on(async {
            let id: Result<Subscription<i32>, _> = self.client.subscribe("state_subscribeStorage", rpc_params![keys], "state_unsubscribeStorage").await;
        });
        sink.send(&());

        Ok(())
    }
}

pub async fn start_relayer_server() -> Result<(SocketAddr, WsServerHandle)> {
    let server = WsServerBuilder::new()
        // .max_connections(100)
        // .max_request_body_size(10)
        // .max_response_body_size(10)
        // .ping_interval(Duration::from_secs(60))
        // .max_subscriptions_per_connection(1024)
        .set_middleware(RelayerLogger)
        .build("127.0.0.1:9988")
        .await?;

    // let full_node = "ws://127.0.0.1:9800";
    let full_node = "wss://ws.rococo.dolphin.engineering:443";
    let client = WsClientBuilder::default().build(&full_node).await?;

    let relayer = MantaRpcRelayServer { client };

    let addr = server.local_addr()?;
    let handle = server.start(relayer.into_rpc())?;
    Ok((addr, handle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_metadata_should_work() {
        let url = "wss://ws.calamari.systems:443";
        assert!(true);
    }
}