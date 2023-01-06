// Copyright 2020-2023 Manta Network.
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

use super::sub_client_pool::MtoMSubClientPool;
use crate::types::Health;
use anyhow::Result;
use jsonrpsee::{
    core::{async_trait, client::ClientT, RpcResult},
    proc_macros::rpc,
    rpc_params,
    types::SubscriptionResult,
    ws_client::WsClient,
    SubscriptionSink,
};
use sc_transaction_pool_api::TransactionStatus;
use sp_core::storage::{StorageChangeSet, StorageData, StorageKey};
use sp_core::Bytes;
use sp_rpc::{list::ListOrValue, number::NumberOrHex};
use sp_runtime::generic::SignedBlock;
use sp_runtime::traits::{BlakeTwo256, IdentifyAccount, Verify};
use std::sync::Arc;

pub type Hash = sp_core::H256;
pub type BlockHash = sp_core::H256;
pub type Block = sp_runtime::generic::Block<Header, sp_runtime::OpaqueExtrinsic>;

/// Arbitrary properties defined in chain spec as a JSON object
pub type Properties = serde_json::map::Map<String, serde_json::Value>;

/// An index to a block.
pub type BlockNumber = u32;

/// Block header type as expected by this runtime.
pub type Header = sp_runtime::generic::Header<BlockNumber, BlakeTwo256>;

pub type AccountId = <<sp_runtime::MultiSignature as Verify>::Signer as IdentifyAccount>::AccountId;

// Nonce
pub type Index = u32;

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

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/system/mod.rs#L50
    #[method(name = "system_properties")]
    async fn system_properties(&self) -> RpcResult<Properties>;

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/system/mod.rs#L56
    #[method(name = "system_health")]
    async fn system_health(&self) -> RpcResult<Health>;

    #[method(name = "system_accountNextIndex", aliases = ["account_nextIndex"])]
    async fn nonce(&self, account: AccountId) -> RpcResult<Index>;

    #[method(name = "chain_getBlock")]
    async fn block(&self, hash: Option<Hash>) -> RpcResult<Option<SignedBlock<Block>>>;

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

    // https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/author/mod.rs#L57
    #[method(name = "author_pendingExtrinsics")]
    async fn pending_extrinsics(&self) -> RpcResult<Vec<Bytes>>;

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
        item = StorageChangeSet<Hash>,
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

    #[subscription(
		name = "author_submitAndWatchExtrinsic" => "author_extrinsicUpdate",
		unsubscribe = "author_unwatchExtrinsic",
		item = TransactionStatus<Hash, BlockHash>,
	)]
    fn watch_extrinsic(&self, bytes: Bytes);
}

#[async_trait]
impl MantaRelayApiServer for MantaRpcRelayServer {
    /// directly sync method

    /// directly async method

    async fn call(&self, name: String, bytes: Bytes, hash: Option<Hash>) -> RpcResult<Bytes> {
        Ok(self
            .dmc
            .request("state_call", rpc_params![name, bytes, hash])
            .await?)
    }

    async fn storage_pairs(
        &self,
        prefix: StorageKey,
        hash: Option<Hash>,
    ) -> RpcResult<Vec<(StorageKey, StorageData)>> {
        Ok(self
            .dmc
            .request("state_getPairs", rpc_params![prefix, hash])
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
                rpc_params![prefix, count, start_key, hash],
            )
            .await?)
    }

    async fn storage(&self, key: StorageKey, hash: Option<Hash>) -> RpcResult<Option<StorageData>> {
        Ok(self
            .dmc
            .request("state_getStorage", rpc_params![key, hash])
            .await?)
    }

    async fn storage_hash(&self, key: StorageKey, hash: Option<Hash>) -> RpcResult<Option<Hash>> {
        Ok(self
            .dmc
            .request("state_getStorageHash", rpc_params![key, hash])
            .await?)
    }

    async fn storage_size(&self, key: StorageKey, hash: Option<Hash>) -> RpcResult<Option<u64>> {
        Ok(self
            .dmc
            .request("state_getStorageSize", rpc_params![key, hash])
            .await?)
    }

    async fn metadata(&self, hash: Option<Hash>) -> RpcResult<Bytes> {
        Ok(self
            .dmc
            .request("state_getMetadata", rpc_params![hash])
            .await?)
    }

    async fn runtime_version(&self, hash: Option<Hash>) -> RpcResult<sp_version::RuntimeVersion> {
        Ok(self
            .dmc
            .request("state_getRuntimeVersion", rpc_params![hash])
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
            .request("state_queryStorage", rpc_params![keys, block, hash])
            .await?)
    }

    async fn query_storage_at(
        &self,
        keys: Vec<StorageKey>,
        at: Option<Hash>,
    ) -> RpcResult<Vec<StorageChangeSet<Hash>>> {
        Ok(self
            .dmc
            .request("state_queryStorageAt", rpc_params![keys, at])
            .await?)
    }

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
                rpc_params![block, targets, storage_keys, methods],
            )
            .await?)
    }

    async fn system_name(&self) -> RpcResult<String> {
        Ok(self.dmc.request("system_name", rpc_params![]).await?)
    }

    async fn system_version(&self) -> RpcResult<String> {
        Ok(self.dmc.request("system_version", rpc_params![]).await?)
    }

    async fn system_chain(&self) -> RpcResult<String> {
        Ok(self.dmc.request("system_chain", rpc_params![]).await?)
    }

    async fn system_properties(&self) -> RpcResult<Properties> {
        Ok(self.dmc.request("system_properties", rpc_params![]).await?)
    }

    async fn system_health(&self) -> RpcResult<Health> {
        Ok(self
            .dmc
            .request::<Health>("system_health", rpc_params![])
            .await?)
    }

    async fn nonce(&self, account: AccountId) -> RpcResult<Index> {
        Ok(self
            .dmc
            .request("system_accountNextIndex", rpc_params![account])
            .await?)
    }

    async fn block(&self, hash: Option<Hash>) -> RpcResult<Option<SignedBlock<Block>>> {
        Ok(self
            .dmc
            .request("chain_getBlock", rpc_params![hash])
            .await?)
    }

    async fn header(&self, hash: Option<Hash>) -> RpcResult<Option<Header>> {
        Ok(self
            .dmc
            .request("chain_getHeader", rpc_params![hash])
            .await?)
    }

    async fn block_hash(
        &self,
        hash: Option<ListOrValue<NumberOrHex>>,
    ) -> RpcResult<ListOrValue<Option<Hash>>> {
        Ok(self
            .dmc
            .request("chain_getBlockHash", rpc_params![hash])
            .await?)
    }

    async fn finalized_head(&self) -> RpcResult<Hash> {
        Ok(self
            .dmc
            .request("chain_getFinalizedHead", rpc_params![])
            .await?)
    }

    async fn submit_extrinsic(&self, extrinsic: Bytes) -> RpcResult<Hash> {
        Ok(self
            .dmc
            .request("author_submitExtrinsic", rpc_params![extrinsic])
            .await?)
    }

    async fn pending_extrinsics(&self) -> RpcResult<Vec<Bytes>> {
        Ok(self
            .dmc
            .request("author_pendingExtrinsics", rpc_params![])
            .await?)
    }

    /// subscription methods.
    /// Subscription methods must not be `async`, this is ruled in marco generation.
    fn subscribe_storage(
        &self,
        sink: SubscriptionSink,
        keys: Option<Vec<StorageKey>>,
    ) -> SubscriptionResult {
        Ok(self.sub_clients.subscribe::<StorageChangeSet<Hash>>(
            sink,
            "state_subscribeStorage",
            rpc_params![keys],
            "state_unsubscribeStorage",
        )?)
    }

    fn subscribe_runtime_version(&self, sink: SubscriptionSink) -> SubscriptionResult {
        Ok(self.sub_clients.subscribe::<sp_version::RuntimeVersion>(
            sink,
            "state_subscribeRuntimeVersion",
            rpc_params![],
            "state_unsubscribeRuntimeVersion",
        )?)
    }

    fn subscribe_all_heads(&self, sink: SubscriptionSink) -> SubscriptionResult {
        Ok(self.sub_clients.subscribe::<Header>(
            sink,
            "chain_subscribeAllHeads",
            rpc_params![],
            "chain_unsubscribeAllHeads",
        )?)
    }

    fn subscribe_new_heads(&self, sink: SubscriptionSink) -> SubscriptionResult {
        Ok(self.sub_clients.subscribe::<Header>(
            sink,
            "chain_subscribeNewHeads",
            rpc_params![],
            "chain_unsubscribeNewHeads",
        )?)
    }

    fn subscribe_finalized_heads(&self, sink: SubscriptionSink) -> SubscriptionResult {
        Ok(self.sub_clients.subscribe::<Header>(
            sink,
            "chain_subscribeFinalizedHeads",
            rpc_params![],
            "chain_unsubscribeFinalizedHeads",
        )?)
    }

    fn watch_extrinsic(&self, sink: SubscriptionSink, bytes: Bytes) -> SubscriptionResult {
        Ok(self
            .sub_clients
            .subscribe::<TransactionStatus<Hash, BlockHash>>(
                sink,
                "author_submitAndWatchExtrinsic",
                rpc_params![bytes],
                "author_unwatchExtrinsic",
            )?)
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
