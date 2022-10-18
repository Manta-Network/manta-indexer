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
use anyhow::{anyhow, Result};
use jsonrpsee::core::client::{ClientT, Subscription, SubscriptionClientT};
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use jsonrpsee::ws_server::{WsServerBuilder, WsServerHandle};
use jsonrpsee::{
    core::{async_trait, Error as JsonRpseeError, RpcResult},
    proc_macros::rpc,
    rpc_params,
    types::{EmptyParams, SubscriptionResult},
    SubscriptionSink,
};
use manta_pay::signer::{Checkpoint as _, RawCheckpoint};
use rusqlite::{Connection, Params};
use sp_core::storage::{StorageChangeSet, StorageData, StorageKey};
use sp_core::Bytes;
use sp_rpc::{list::ListOrValue, number::NumberOrHex};
use sp_runtime::traits::BlakeTwo256;
use std::net::SocketAddr;
use std::ops::Sub;
use std::sync::Arc;
use std::time::Duration;
use frame_support::log::error;
use jsonrpsee::types::error::CallError;
use jsonrpsee::types::SubscriptionEmptyError;
use tokio::sync::Mutex;
use crate::relaying::sub_client_pool::{INIT_RUNTIME, MtoMSubClientPool};

pub type Hash = sp_core::H256;

/// Arbitrary properties defined in chain spec as a JSON object
pub type Properties = serde_json::map::Map<String, serde_json::Value>;

/// An index to a block.
pub type BlockNumber = u32;

/// Block header type as expected by this runtime.
pub type Header = sp_runtime::generic::Header<BlockNumber, BlakeTwo256>;

/// The whole relaying server implementation.
pub struct MantaRpcRelayServer {
    pub backend_uri: String,

    // dmc = directly_method_client, use this client to relay all
    // sync and async method, we manage the subscription method in other single field.
    // TODO make it generic and wrap a pooling client.
    pub dmc: Arc<WsClient>,

    // use this client pool to manage all subscription connection.
    // version 1: m to m. each sub go with a new single conn.
    // TODO version 2: m to 1, all sub go with single conn.
    // TODO version 3: m to n, all sub go with a pool with n conn.
    // TODO make it generic.
    pub sub_clients: Arc<MtoMSubClientPool>,
}

/// The whole relaying
pub(crate) struct MantaRpcRelayClient {}

/// MantaRelayApi declare the relaying part of Indexer.
/// Each rpc method below is actually also declared in full node and used by our DApp.
///
/// You can trace all available rpc_api at:
/// https://github.com/paritytech/substrate/tree/master/client/rpc-api
///
/// for example state api is:
/// https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/state/mod.rs
///
/// the detail usage of `rpc(client, server)` is:
/// https://docs.rs/jsonrpsee-proc-macros/0.15.1/jsonrpsee_proc_macros/attr.rpc.html
#[rpc(client, server)]
pub trait MantaRelayApi {
    /// sync fn declaration.

    /// async fn declaration.

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/state/mod.rs#L78
    // NOTE: origin has a generic `hash` param but not sure the usage, just ignore to forbidden parse error.
    // trait bounds: https://github.com/paritytech/substrate/blob/master/utils/frame/rpc/support/src/lib.rs#L181
    #[method(name = "state_getMetadata")]
    async fn metadata(&self) -> RpcResult<Bytes>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/state/mod.rs#L99
    #[method(name = "state_queryStorageAt")]
    async fn query_storage_at(
        &self,
        keys: Vec<StorageKey>,
        at: Option<Hash>,
    ) -> RpcResult<Vec<StorageChangeSet<Hash>>>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/state/mod.rs#L37
    #[method(name = "state_call", aliases = ["state_callAt"])]
    async fn call(&self, name: String, bytes: Bytes, hash: Option<Hash>) -> RpcResult<Bytes>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/state/mod.rs#L45
    #[method(name = "state_getPairs")]
    async fn storage_pairs(
        &self,
        prefix: StorageKey,
        hash: Option<Hash>,
    ) -> RpcResult<Vec<(StorageKey, StorageData)>>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/state/mod.rs#L53
    #[method(name = "state_getKeysPaged", aliases = ["state_getKeysPagedAt"])]
    async fn storage_keys_paged(
        &self,
        prefix: Option<StorageKey>,
        count: u32,
        start_key: Option<StorageKey>,
        hash: Option<Hash>,
    ) -> RpcResult<Vec<StorageKey>>;

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

    #[method(name = "system_health")]
    async fn system_health(&self) -> RpcResult<Health>;

    #[method(name = "chain_getHeader")]
    async fn header(&self, hash: Option<Hash>) -> RpcResult<Option<Header>>;

    #[method(name = "author_submitExtrinsic")]
    async fn submit_extrinsic(&self, extrinsic: Bytes) -> RpcResult<Hash>;

    #[method(name = "chain_getFinalizedHead", aliases = ["chain_getFinalisedHead"])]
    async fn finalized_head(&self) -> RpcResult<Hash>;

    /// subscription fn declaration.
    #[subscription(
    name = "state_subscribeRuntimeVersion" => "state_runtimeVersion",
    unsubscribe = "state_unsubscribeRuntimeVersion",
    aliases = ["chain_subscribeRuntimeVersion"],
    unsubscribe_aliases = ["chain_unsubscribeRuntimeVersion"],
    item = sp_version::RuntimeVersion,
    )]
    fn subscribe_runtime_version(&self);

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/state/mod.rs#L120
    #[subscription(
    name = "state_subscribeStorage" => "state_storage",
    unsubscribe = "state_unsubscribeStorage",
    item = StorageChangeSet < Hash >,
    )]
    fn subscribe_storage(&self, keys: Option<Vec<StorageKey>>);

    #[subscription(name = "sub" => "subNotif", unsubscribe = "unsub", item = String)]
    fn sub_override_notif_method(&self);

    #[subscription(name = "subscribe", item = String)]
    fn sub(&self);
}

#[async_trait]
impl MantaRelayApiServer for MantaRpcRelayServer {
    /// directly sync method


    /// directly async method
    async fn metadata(&self) -> RpcResult<Bytes> {
        Ok(self.dmc.request("state_getMetadata", None).await?)
    }

    async fn query_storage_at(
        &self,
        keys: Vec<StorageKey>,
        at: Option<Hash>,
    ) -> RpcResult<Vec<StorageChangeSet<Hash>>> {
        Ok(self.dmc.request("state_queryStorageAt", rpc_params!(keys, at)).await?)
    }

    async fn call(&self, name: String, bytes: Bytes, hash: Option<Hash>) -> RpcResult<Bytes> {
        Ok(self.dmc.request("state_call", rpc_params!(name, bytes, hash)).await?)
    }

    async fn storage_pairs(
        &self,
        prefix: StorageKey,
        hash: Option<Hash>,
    ) -> RpcResult<Vec<(StorageKey, StorageData)>> {
        Ok(self.dmc.request("state_getPairs", rpc_params!(prefix, hash)).await?)
    }

    async fn storage_keys_paged(
        &self,
        prefix: Option<StorageKey>,
        count: u32,
        start_key: Option<StorageKey>,
        hash: Option<Hash>,
    ) -> RpcResult<Vec<StorageKey>> {
        Ok(self.dmc.request("state_getKeysPaged", rpc_params!(prefix, count, start_key, hash)).await?)
    }


    // now

    fn sub_override_notif_method(&self, mut sink: SubscriptionSink) -> SubscriptionResult {
        tokio::spawn(async move {
            let stream = tokio_stream::iter(["one", "two", "three"]);
            sink.pipe_from_stream(stream).await;
        });
        Ok(())
    }

    fn sub(&self, mut sink: SubscriptionSink) -> SubscriptionResult {
        let _ = sink.send(&"Response_A");
        let _ = sink.send(&"Response_B");
        Ok(())
    }


    async fn block_hash(
        &self,
        hash: Option<ListOrValue<NumberOrHex>>,
    ) -> RpcResult<ListOrValue<Option<Hash>>> {
        let _hash = hash.map(|h| rpc_params![h]).unwrap_or(rpc_params![]);
        let block = self
            .dmc
            .request::<ListOrValue<Option<Hash>>>("chain_getBlockHash", _hash)
            .await?;

        Ok(block)
    }

    async fn runtime_version(&self, hash: Option<Hash>) -> RpcResult<sp_version::RuntimeVersion> {
        let _hash = hash.map(|h| rpc_params![h]).unwrap_or(rpc_params![]);
        let rt_version = self
            .dmc
            .request::<sp_version::RuntimeVersion>("state_getRuntimeVersion", _hash)
            .await?;

        Ok(rt_version)
    }

    async fn system_chain(&self) -> RpcResult<String> {
        let chain = self.dmc.request::<String>("system_chain", None).await?;

        Ok(chain)
    }

    async fn system_properties(&self) -> RpcResult<Properties> {
        let properties = self
            .dmc
            .request::<Properties>("system_properties", None)
            .await?;

        Ok(properties)
    }

    async fn rpc_methods(&self) -> RpcResult<RpcMethods> {
        let methods = self
            .dmc
            .request::<RpcMethods>("rpc_methods", None)
            .await?;

        Ok(methods)
    }

    fn subscribe_runtime_version(&self, mut sink: SubscriptionSink) -> SubscriptionResult {
        let client = self.dmc.clone();
        tokio::spawn(async move {
            match client
                .request::<sp_version::RuntimeVersion>("state_getRuntimeVersion", None)
                .await
            {
                Ok(version) => sink
                    .send(&version)
                    .map_err(|e| JsonRpseeError::Custom(e.to_string())),
                Err(e) => Err(e),
            }
        });

        Ok(())
    }

    /// subscription methods.
    /// Subscription methods must not be `async`

    fn subscribe_storage(
        &self,
        mut sink: SubscriptionSink,
        keys: Option<Vec<StorageKey>>,
    ) -> SubscriptionResult {
        let key = sink.sub_keys().ok_or_else(|| {
            error!("{} get a subscription error, call before accept", sink.method_name());
            anyhow!("")
        })?;

        let client;
        if !self.sub_clients.clients.contains_key(&key.conn_id) {
            let uri = self.backend_uri.as_str();
            match INIT_RUNTIME.block_on(WsClientBuilder::default().build(uri)) {
                Ok(cli) => client = Arc::new(cli),
                Err(e) => return Err(SubscriptionEmptyError)
            }
            self.sub_clients.clients.insert(key.conn_id, client.clone());
        } else {
            client = self.sub_clients.clients.get(&key.conn_id).unwrap().clone();
        }

        let params = rpc_params!([keys]);
        tokio::spawn(async move {
            match client.subscribe::<StorageChangeSet<Hash>>("state_subscribeStorage", params, "state_unsubscribeStorage").await {
                Ok(mut channel) => {
                    // build a pipeline, client receive a message from full node and send to sink.
                    while let Some(msg) = channel.next().await {
                        match msg {
                            Ok(mut inner) => {
                                let _ = sink.send(&mut inner);
                            }
                            Err(e) => {
                                error!("subscribeStorage get some error: {:?}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    // close the upstream subscription channel by error.
                    sink.close(CallError::Failed(anyhow!("{:?}", e)));
                }
            }
        });
        Ok(())
    }

    async fn system_health(&self) -> RpcResult<Health> {
        let health = self.dmc.request::<Health>("system_health", None).await?;

        Ok(health)
    }


    async fn header(&self, hash: Option<Hash>) -> RpcResult<Option<Header>> {
        let _hash = hash.map(|h| rpc_params![h]).unwrap_or(rpc_params![]);
        let header = self
            .dmc
            .request::<Option<Header>>("chain_getHeader", _hash)
            .await?;

        Ok(header)
    }

    async fn submit_extrinsic(&self, extrinsic: Bytes) -> RpcResult<Hash> {
        let hash = self
            .dmc
            .request::<Hash>("author_submitExtrinsic", rpc_params![extrinsic])
            .await?;

        Ok(hash)
    }

    async fn finalized_head(&self) -> RpcResult<Hash> {
        let hash = self
            .dmc
            .request::<Hash>("chain_getFinalizedHead", None)
            .await?;

        Ok(hash)
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

    let full_node = "ws://127.0.0.1:9800";
    let full_node = "wss://ws.rococo.dolphin.engineering:443";
    let client = WsClientBuilder::default().build(&full_node).await?;

    let relayer = MantaRpcRelayServer {
        backend_uri: full_node.to_string(),
        dmc: Arc::new(client),
        sub_clients: Arc::new(Default::default()),
    };

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

    #[tokio::test]
    async fn subscriber_should_work() {
        start_relayer_server().await;
        let url = "ws://127.0.0.1:9988";
        let client = WsClientBuilder::default().build(&url).await.unwrap();

        let mut sub = client.sub().await.unwrap();
        let first_recv = sub.next().await.unwrap().unwrap();
        assert_eq!(first_recv, "Response_A".to_string());
        let second_recv = sub.next().await.unwrap().unwrap();
        assert_eq!(second_recv, "Response_B".to_string());
    }
}
