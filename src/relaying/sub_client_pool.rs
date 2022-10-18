use std::sync::Arc;
use dashmap::DashMap;
use jsonrpsee::core::client::{ClientT, Subscription, SubscriptionClientT};
use jsonrpsee::core::Error;
use jsonrpsee::core::server::rpc_module::ConnectionId;
use jsonrpsee::types::ParamsSer;
use jsonrpsee::ws_client::WsClient;
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
use async_trait::async_trait;


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
    pub clients: DashMap<ConnectionId, Arc<WsClient>>,
}


#[async_trait]
impl ClientT for MtoMSubClientPool {
    async fn notification<'a>(&self, method: &'a str, params: Option<ParamsSer<'a>>) -> Result<(), Error> {
        todo!()
    }

    async fn request<'a, R>(&self, method: &'a str, params: Option<ParamsSer<'a>>) -> Result<R, Error> where R: DeserializeOwned {
        todo!()
    }

    async fn batch_request<'a, R>(&self, batch: Vec<(&'a str, Option<ParamsSer<'a>>)>) -> Result<Vec<R>, Error> where R: DeserializeOwned + Default + Clone {
        todo!()
    }
}

#[async_trait]
impl SubscriptionClientT for MtoMSubClientPool {
    async fn subscribe<'a, Notif>(&self, subscribe_method: &'a str, params: Option<ParamsSer<'a>>, unsubscribe_method: &'a str) -> Result<Subscription<Notif>, Error> where Notif: DeserializeOwned {
        todo!()
    }

    async fn subscribe_to_method<'a, Notif>(&self, method: &'a str) -> Result<Subscription<Notif>, Error> where Notif: DeserializeOwned {
        todo!()
    }
}
