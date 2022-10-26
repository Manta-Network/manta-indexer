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

use super::{middleware::IndexerMiddleware, sub_client_pool::MtoMSubClientPool};
use crate::logger::RelayerLogger;
use crate::types::{Health, RpcMethods};
use anyhow::Result;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use jsonrpsee::ws_server::{WsServerBuilder, WsServerHandle};
use jsonrpsee::{
    core::{async_trait, RpcResult},
    proc_macros::rpc,
    rpc_params,
    types::SubscriptionResult,
    SubscriptionSink,
};
use sp_core::storage::{StorageChangeSet, StorageData, StorageKey};
use sp_core::Bytes;
use sp_rpc::{list::ListOrValue, number::NumberOrHex};
use sp_runtime::traits::BlakeTwo256;
use std::net::SocketAddr;
use std::sync::Arc;

pub type Hash = sp_core::H256;

/// Arbitrary properties defined in chain spec as a JSON object
pub type Properties = serde_json::map::Map<String, serde_json::Value>;

/// An index to a block.
pub type BlockNumber = u32;

/// Block header type as expected by this runtime.
pub type Header = sp_runtime::generic::Header<BlockNumber, BlakeTwo256>;

/// The whole relaying server implementation.
pub struct MantaRpcRelayServer {
    // pub backend_uri: String,

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

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/state/mod.rs#L65
    #[method(name = "state_getStorage", aliases = ["state_getStorageAt"])]
    async fn storage(&self, key: StorageKey, hash: Option<Hash>) -> RpcResult<Option<StorageData>>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/state/mod.rs#L69
    #[method(name = "state_getStorageHash", aliases = ["state_getStorageHashAt"])]
    async fn storage_hash(&self, key: StorageKey, hash: Option<Hash>) -> RpcResult<Option<Hash>>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/state/mod.rs#L73
    #[method(name = "state_getStorageSize", aliases = ["state_getStorageSizeAt"])]
    async fn storage_size(&self, key: StorageKey, hash: Option<Hash>) -> RpcResult<Option<u64>>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/state/mod.rs#L78
    #[method(name = "state_getMetadata")]
    async fn metadata(&self, hash: Option<Hash>) -> RpcResult<Bytes>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/state/mod.rs#L81
    #[method(name = "state_getRuntimeVersion", aliases = ["chain_getRuntimeVersion"])]
    async fn runtime_version(&self, hash: Option<Hash>) -> RpcResult<sp_version::RuntimeVersion>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/state/mod.rs#L85
    #[method(name = "state_queryStorage")]
    async fn query_storage(
        &self,
        keys: Vec<StorageKey>,
        block: Hash,
        hash: Option<Hash>,
    ) -> RpcResult<Vec<StorageChangeSet<Hash>>>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/state/mod.rs#L99
    #[method(name = "state_queryStorageAt")]
    async fn query_storage_at(
        &self,
        keys: Vec<StorageKey>,
        at: Option<Hash>,
    ) -> RpcResult<Vec<StorageChangeSet<Hash>>>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/state/mod.rs#L106
    // #[method(name = "state_getReadProof")]
    // async fn read_proof(&self, keys: Vec<StorageKey>, hash: Option<Hash>) -> RpcResult<ReadProof<Hash>>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/state/mod.rs#L280
    #[method(name = "state_traceBlock")]
    async fn trace_block(
        &self,
        block: Hash,
        targets: Option<String>,
        storage_keys: Option<String>,
        methods: Option<String>,
    ) -> RpcResult<sp_rpc::tracing::TraceBlockResponse>;
    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/system/mod.rs#L34
    #[method(name = "system_name")]
    async fn system_name(&self) -> RpcResult<String>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/system/mod.rs#L38
    #[method(name = "system_version")]
    async fn system_version(&self) -> RpcResult<String>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/system/mod.rs#L42
    #[method(name = "system_chain")]
    async fn system_chain(&self) -> RpcResult<String>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/system/mod.rs#L46
    // #[method(name = "system_chainType")]
    // async fn system_type(&self) -> RpcResult<sc_chain_spec::ChainType>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/system/mod.rs#L50
    #[method(name = "system_properties")]
    async fn system_properties(&self) -> RpcResult<Properties>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/system/mod.rs#L56
    #[method(name = "system_health")]
    async fn system_health(&self) -> RpcResult<Health>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/system/mod.rs#L62
    #[method(name = "system_localPeerId")]
    async fn system_local_peer_id(&self) -> RpcResult<String>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/system/mod.rs#L66
    #[method(name = "system_localListenAddresses")]
    async fn system_local_listen_addresses(&self) -> RpcResult<Vec<String>>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/system/mod.rs#L86
    #[method(name = "system_addReservedPeer")]
    async fn system_add_reserved_peer(&self, peer: String) -> RpcResult<()>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/system/mod.rs#L94
    #[method(name = "system_removeReservedPeer")]
    async fn system_remove_reserved_peer(&self, peer_id: String) -> RpcResult<()>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/system/mod.rs#L99
    #[method(name = "system_reservedPeers")]
    async fn system_reserved_peers(&self) -> RpcResult<Vec<String>>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/system/mod.rs#L112
    #[method(name = "system_addLogFilter")]
    async fn system_add_log_filter(&self, directives: String) -> RpcResult<()>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/system/mod.rs#L120
    #[method(name = "system_resetLogFilter")]
    async fn system_reset_log_filter(&self) -> RpcResult<()>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/chain/mod.rs#L28
    #[method(name = "chain_getHeader")]
    async fn header(&self, hash: Option<Hash>) -> RpcResult<Option<Header>>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/chain/mod.rs#L36
    #[method(name = "chain_getBlockHash", aliases = ["chain_getHead"])]
    async fn block_hash(
        &self,
        hash: Option<ListOrValue<NumberOrHex>>,
    ) -> RpcResult<ListOrValue<Option<Hash>>>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/chain/mod.rs#L45
    #[method(name = "chain_getFinalizedHead", aliases = ["chain_getFinalisedHead"])]
    async fn finalized_head(&self) -> RpcResult<Hash>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/author/mod.rs#L31
    #[method(name = "author_submitExtrinsic")]
    async fn submit_extrinsic(&self, extrinsic: Bytes) -> RpcResult<Hash>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/author/mod.rs#L35
    #[method(name = "author_insertKey")]
    async fn insert_key(&self, key_type: String, suri: String, public: Bytes) -> RpcResult<()>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/author/mod.rs#L39
    #[method(name = "author_rotateKeys")]
    async fn rotate_keys(&self) -> RpcResult<Bytes>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/author/mod.rs#L43
    #[method(name = "author_hasSessionKeys")]
    async fn has_session_keys(&self, session_keys: Bytes) -> RpcResult<bool>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/author/mod.rs#L51
    #[method(name = "author_hasKey")]
    async fn has_key(&self, public_key: Bytes, key_type: String) -> RpcResult<bool>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/author/mod.rs#L57
    #[method(name = "author_pendingExtrinsics")]
    async fn pending_extrinsics(&self) -> RpcResult<Vec<Bytes>>;

    // attached by framework.
    #[method(name = "rpc_methods")]
    async fn rpc_methods(&self) -> RpcResult<RpcMethods>;

    /// subscription fn declaration.

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/state/mod.rs#L110
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

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/chain/mod.rs#L49
    #[subscription(
    name = "chain_subscribeAllHeads" => "chain_allHead",
    unsubscribe = "chain_unsubscribeAllHeads",
    item = Header
    )]
    fn subscribe_all_heads(&self);

    // New head subscription.
    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/chain/mod.rs#L58
    #[subscription(
    name = "chain_subscribeNewHeads" => "chain_newHead",
    aliases = ["subscribe_newHead", "chain_subscribeNewHead"],
    unsubscribe = "chain_unsubscribeNewHeads",
    unsubscribe_aliases = ["unsubscribe_newHead", "chain_unsubscribeNewHead"],
    item = Header
    )]
    fn subscribe_new_heads(&self);

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/chain/mod.rs#L67
    #[subscription(
    name = "chain_subscribeFinalizedHeads" => "chain_finalizedHead",
    aliases = ["chain_subscribeFinalisedHeads"],
    unsubscribe = "chain_unsubscribeFinalizedHeads",
    unsubscribe_aliases = ["chain_unsubscribeFinalisedHeads"],
    item = Header
    )]
    fn subscribe_finalized_heads(&self);

    // a test debug subscription
    #[subscription(name = "sub" => "subNotif", unsubscribe = "unsub", item = String)]
    fn sub_override_notif_method(&self);

    // a test debug method
    #[subscription(name = "subscribe", item = String)]
    fn sub(&self);
}

#[async_trait]
impl MantaRelayApiServer for MantaRpcRelayServer {
    /// directly sync method

    /// directly async method

    async fn call(&self, name: String, bytes: Bytes, hash: Option<Hash>) -> RpcResult<Bytes> {
        Ok(self
            .dmc
            .request("state_call", rpc_params!(name, bytes, hash))
            .await?)
    }

    async fn storage_pairs(
        &self,
        prefix: StorageKey,
        hash: Option<Hash>,
    ) -> RpcResult<Vec<(StorageKey, StorageData)>> {
        Ok(self
            .dmc
            .request("state_getPairs", rpc_params!(prefix, hash))
            .await?)
    }

    async fn storage_keys_paged(
        &self,
        prefix: Option<StorageKey>,
        count: u32,
        start_key: Option<StorageKey>,
        hash: Option<Hash>,
    ) -> RpcResult<Vec<StorageKey>> {
        Ok(self
            .dmc
            .request(
                "state_getKeysPaged",
                rpc_params!(prefix, count, start_key, hash),
            )
            .await?)
    }

    async fn storage(&self, key: StorageKey, hash: Option<Hash>) -> RpcResult<Option<StorageData>> {
        Ok(self
            .dmc
            .request("state_getStorage", rpc_params!(key, hash))
            .await?)
    }

    async fn storage_hash(&self, key: StorageKey, hash: Option<Hash>) -> RpcResult<Option<Hash>> {
        Ok(self
            .dmc
            .request("state_getStorageHash", rpc_params!(key, hash))
            .await?)
    }

    async fn storage_size(&self, key: StorageKey, hash: Option<Hash>) -> RpcResult<Option<u64>> {
        Ok(self
            .dmc
            .request("state_getStorageSize", rpc_params!(key, hash))
            .await?)
    }

    async fn metadata(&self, hash: Option<Hash>) -> RpcResult<Bytes> {
        Ok(self.dmc.request("state_getMetadata", None).await?)
    }

    async fn runtime_version(&self, hash: Option<Hash>) -> RpcResult<sp_version::RuntimeVersion> {
        Ok(self
            .dmc
            .request("state_getRuntimeVersion", rpc_params!(hash))
            .await?)
    }

    async fn query_storage(
        &self,
        keys: Vec<StorageKey>,
        block: Hash,
        hash: Option<Hash>,
    ) -> RpcResult<Vec<StorageChangeSet<Hash>>> {
        Ok(self
            .dmc
            .request("state_queryStorage", rpc_params!(keys, block, hash))
            .await?)
    }

    async fn query_storage_at(
        &self,
        keys: Vec<StorageKey>,
        at: Option<Hash>,
    ) -> RpcResult<Vec<StorageChangeSet<Hash>>> {
        Ok(self
            .dmc
            .request("state_queryStorageAt", rpc_params!(keys, at))
            .await?)
    }

    // async fn read_proof(&self, keys: Vec<StorageKey>, hash: Option<Hash>) -> RpcResult<ReadProof<Hash>> {
    //     Ok(self.dmc.request("state_getReadProof", rpc_params!(keys, hash)).await?)
    // }

    async fn trace_block(
        &self,
        block: Hash,
        targets: Option<String>,
        storage_keys: Option<String>,
        methods: Option<String>,
    ) -> RpcResult<sp_rpc::tracing::TraceBlockResponse> {
        Ok(self
            .dmc
            .request(
                "state_traceBlock",
                rpc_params!(block, targets, storage_keys, methods),
            )
            .await?)
    }

    async fn system_name(&self) -> RpcResult<String> {
        Ok(self.dmc.request("system_name", None).await?)
    }

    async fn system_version(&self) -> RpcResult<String> {
        Ok(self.dmc.request("system_version", None).await?)
    }

    async fn system_chain(&self) -> RpcResult<String> {
        Ok(self.dmc.request("system_chain", None).await?)
    }

    async fn system_properties(&self) -> RpcResult<Properties> {
        Ok(self.dmc.request("system_properties", None).await?)
    }

    async fn system_health(&self) -> RpcResult<Health> {
        Ok(self.dmc.request::<Health>("system_health", None).await?)
    }

    async fn system_local_peer_id(&self) -> RpcResult<String> {
        Ok(self.dmc.request("system_localPeerId", None).await?)
    }

    async fn system_local_listen_addresses(&self) -> RpcResult<Vec<String>> {
        Ok(self
            .dmc
            .request("system_localListenAddresses", None)
            .await?)
    }

    async fn system_add_reserved_peer(&self, peer: String) -> RpcResult<()> {
        Ok(self
            .dmc
            .request("system_addReservedPeer", rpc_params!(peer))
            .await?)
    }

    async fn system_remove_reserved_peer(&self, peer_id: String) -> RpcResult<()> {
        Ok(self
            .dmc
            .request("system_removeReservedPeer", rpc_params!(peer_id))
            .await?)
    }

    async fn system_reserved_peers(&self) -> RpcResult<Vec<String>> {
        Ok(self.dmc.request("system_reservedPeers", None).await?)
    }

    async fn system_add_log_filter(&self, directives: String) -> RpcResult<()> {
        Ok(self
            .dmc
            .request("system_addLogFilter", rpc_params!(directives))
            .await?)
    }

    async fn system_reset_log_filter(&self) -> RpcResult<()> {
        Ok(self.dmc.request("system_resetLogFilter", None).await?)
    }

    async fn header(&self, hash: Option<Hash>) -> RpcResult<Option<Header>> {
        Ok(self
            .dmc
            .request("chain_getHeader", rpc_params!(hash))
            .await?)
    }

    async fn block_hash(
        &self,
        hash: Option<ListOrValue<NumberOrHex>>,
    ) -> RpcResult<ListOrValue<Option<Hash>>> {
        Ok(self
            .dmc
            .request("chain_getBlockHash", rpc_params!(hash))
            .await?)
    }

    async fn finalized_head(&self) -> RpcResult<Hash> {
        Ok(self.dmc.request("chain_getFinalizedHead", None).await?)
    }

    async fn submit_extrinsic(&self, extrinsic: Bytes) -> RpcResult<Hash> {
        Ok(self
            .dmc
            .request("author_submitExtrinsic", rpc_params![extrinsic])
            .await?)
    }

    async fn insert_key(&self, key_type: String, suri: String, public: Bytes) -> RpcResult<()> {
        Ok(self
            .dmc
            .request("author_insertKey", rpc_params!(key_type, suri, public))
            .await?)
    }

    async fn rotate_keys(&self) -> RpcResult<Bytes> {
        Ok(self.dmc.request("author_rotateKeys", None).await?)
    }

    async fn has_session_keys(&self, session_keys: Bytes) -> RpcResult<bool> {
        Ok(self
            .dmc
            .request("author_hasSessionKeys", rpc_params!(session_keys))
            .await?)
    }

    async fn has_key(&self, public_key: Bytes, key_type: String) -> RpcResult<bool> {
        Ok(self
            .dmc
            .request("author_hasKey", rpc_params!(public_key, key_type))
            .await?)
    }

    async fn pending_extrinsics(&self) -> RpcResult<Vec<Bytes>> {
        Ok(self.dmc.request("author_pendingExtrinsics", None).await?)
    }

    async fn rpc_methods(&self) -> RpcResult<RpcMethods> {
        let methods = self.dmc.request::<RpcMethods>("rpc_methods", None).await?;

        Ok(methods)
    }

    /// subscription methods.
    /// Subscription methods must not be `async`, this is ruled in marco generation.
    fn subscribe_storage(
        &self,
        mut sink: SubscriptionSink,
        keys: Option<Vec<StorageKey>>,
    ) -> SubscriptionResult {
        Ok(self.sub_clients.subscribe::<StorageChangeSet<Hash>>(
            sink,
            "state_subscribeStorage",
            rpc_params!([keys]),
            "state_unsubscribeStorage",
        )?)
    }

    fn subscribe_runtime_version(&self, mut sink: SubscriptionSink) -> SubscriptionResult {
        Ok(self.sub_clients.subscribe::<sp_version::RuntimeVersion>(
            sink,
            "state_subscribeRuntimeVersion",
            None,
            "state_unsubscribeRuntimeVersion",
        )?)
    }

    fn subscribe_all_heads(&self, mut sink: SubscriptionSink) -> SubscriptionResult {
        Ok(self.sub_clients.subscribe::<Header>(
            sink,
            "chain_subscribeAllHeads",
            None,
            "chain_unsubscribeAllHeads",
        )?)
    }

    fn subscribe_new_heads(&self, mut sink: SubscriptionSink) -> SubscriptionResult {
        Ok(self.sub_clients.subscribe::<Header>(
            sink,
            "chain_subscribeNewHeads",
            None,
            "chain_unsubscribeNewHeads",
        )?)
    }

    fn subscribe_finalized_heads(&self, mut sink: SubscriptionSink) -> SubscriptionResult {
        Ok(self.sub_clients.subscribe::<Header>(
            sink,
            "chain_subscribeFinalizedHeads",
            None,
            "chain_unsubscribeFinalizedHeads",
        )?)
    }

    fn sub(&self, mut sink: SubscriptionSink) -> SubscriptionResult {
        let _ = sink.send(&"Response_A");
        let _ = sink.send(&"Response_B");
        Ok(())
    }

    fn sub_override_notif_method(&self, mut sink: SubscriptionSink) -> SubscriptionResult {
        tokio::spawn(async move {
            let stream = tokio_stream::iter(["one", "two", "three"]);
            sink.pipe_from_stream(stream).await;
        });
        Ok(())
    }
}

impl MantaRpcRelayServer {
    pub async fn new(full_node: &str) -> Result<Self> {
        let client = crate::utils::create_ws_client(full_node).await?;
        let relayer = MantaRpcRelayServer {
            dmc: Arc::new(client),
            sub_clients: Arc::new(MtoMSubClientPool::new(full_node.to_string())),
        };

        Ok(relayer)
    }
}

// pub async fn start_relayer_server() -> Result<(SocketAddr, WsServerHandle)> {
//     let server = WsServerBuilder::new()
//         // .max_connections(100)
//         // .max_request_body_size(10)
//         // .max_response_body_size(10)
//         // .ping_interval(Duration::from_secs(60))
//         // .max_subscriptions_per_connection(1024)
//         .set_middleware(IndexerMiddleware::default())
//         .build("127.0.0.1:9988")
//         .await?;

//     // let full_node = "ws://127.0.0.1:9800";
//     let full_node = "wss://ws.rococo.dolphin.engineering:443";
//     let client = crate::utils::create_ws_client(full_node).await?;

//     let relayer = MantaRpcRelayServer {
//         // backend_uri: full_node.to_string(),
//         dmc: Arc::new(client),
//         sub_clients: Arc::new(MtoMSubClientPool::new(full_node.to_string())),
//     };

//     let addr = server.local_addr()?;
//     let handle = server.start(relayer.into_rpc())?;
//     Ok((addr, handle))
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_metadata_should_work() {
        let url = "wss://ws.calamari.systems:443";
        assert!(true);
    }
}
