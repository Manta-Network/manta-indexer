use std::sync::Arc;
use anyhow::{anyhow, bail};
use dashmap::DashMap;
use jsonrpsee::core::client::{ClientT, Subscription, SubscriptionClientT};
use jsonrpsee::core::Error;
use jsonrpsee::core::server::rpc_module::{ConnectionId, SubscriptionKey};
use jsonrpsee::types::{ErrorObjectOwned, ParamsSer};
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
use async_trait::async_trait;
use frame_support::log::error;
use futures::{FutureExt, TryFutureExt, TryStreamExt, StreamExt};
use jsonrpsee::async_client::Client;
use jsonrpsee::{rpc_params, SubscriptionSink};
use jsonrpsee::core::error::SubscriptionClosed;
use serde::Serialize;


/// This runtime pool is distinguished with main env, and used for some hacky need.
/// The most important reason:
///     we need to call some async init function in sync semantic env.
///     As a relaying part, we will build new resource when some request comes,
///     and we support some sync methods like sync call and subscription,
///     and those resource initialization is async function, so there's a hack.
///     We can't just call it here because the async runtime doesn't allow a sync way.
///     So we create a new tiny runtime to call a `block_on` method to act as a sync.
pub(crate) static INIT_RUNTIME: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("relay-init-pool")
        .worker_threads(2)
        .build()
        .unwrap()
});

/// Here's version 1 subscription indexer relaying client.
/// It manage subscription with a M to M mapping. each new subscription **from a single Dapp connection**
/// will go with a new connection to backend full node.
#[derive(Default)]
pub struct MtoMSubClientPool {
    // full_node connecting uri.
    pub backend_uri: String,
    // all managed ws client, each ConnId use one connection with multiplexing subscription.
    pub clients: DashMap<ConnectionId, Arc<WsClient>>,
}


impl MtoMSubClientPool {
    /// get a client by connId.
    fn get_client(&self, key: SubscriptionKey) -> anyhow::Result<Arc<Client>> {
        let client;
        if !self.clients.contains_key(&key.conn_id) {
            let uri = self.backend_uri.as_str();
            match INIT_RUNTIME.block_on(WsClientBuilder::default().build(uri)) {
                Ok(cli) => client = Arc::new(cli),
                Err(e) => bail!("MtoM client creation fail {:?}", e)
            }
            self.clients.insert(key.conn_id, client.clone());
        } else {
            client = self.clients.get(&key.conn_id).unwrap().clone();
        }
        Ok(client)
    }

    /// Here `subscribe` function deal with the internal sinking transform logic.
    /// It creates a subscription from downstream full node and relay data to upstream Dapp sink.
    /// Args:
    ///     * `sink`: upstream Dapp connection sinking channel.
    ///     * `sub_method`: subscription method name.
    ///     * `params`: the params need to build up downstream subscription.
    ///     * `unsub_method`: unsubscription method name.
    /// Template:
    ///     * `N`: received subscription data type.
    pub fn subscribe<'a, N>(&self, mut sink: SubscriptionSink, sub_method: &'a str, params: Option<ParamsSer<'a>>, unsub_method: &'a str) -> anyhow::Result<()> where
        N: DeserializeOwned + Serialize + Send + 'static {
        let key = sink.sub_keys().ok_or_else(|| {
            error!("{} get a subscription error, call before accept", sink.method_name());
            anyhow!("")
        })?;
        let client = self.get_client(key)?;

        // here means downstream subscription has been built successfully.
        let sub_channel = INIT_RUNTIME.block_on(async move {
            client.subscribe::<N>(sub_method, params, unsub_method).await
        })?;
        let sub_channel = sub_channel.filter_map(|msg| async {
            match msg {
                Ok(m) => Some(m),
                Err(e) => None
            }
        });
        let sub_channel = Box::pin(sub_channel);

        tokio::spawn(async move {
            match sink.pipe_from_stream(sub_channel).await {
                SubscriptionClosed::Success => {
                    let err_obj: ErrorObjectOwned = SubscriptionClosed::Success.into();
                    sink.close(err_obj);
                }
                SubscriptionClosed::RemotePeerAborted => {}
                SubscriptionClosed::Failed(e) => { sink.close(e); }
            }
        });
        Ok(())
    }
}

