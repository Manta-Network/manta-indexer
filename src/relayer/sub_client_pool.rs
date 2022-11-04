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

use anyhow::anyhow;
use dashmap::DashMap;
use frame_support::log::{error, trace};
use futures::StreamExt;
use jsonrpsee::{
    async_client::Client,
    core::{
        client::SubscriptionClientT,
        error::SubscriptionClosed,
        server::rpc_module::{ConnectionId, SubscriptionKey},
    },
    types::{ErrorObjectOwned, ParamsSer},
    ws_client::{WsClient, WsClientBuilder},
    SubscriptionSink,
};
use serde::{Deserialize, Serialize};
use std::{future::Future, sync::Arc, time::Duration};

type OnceJob = Arc<dyn Send + Sync + Fn() -> std::pin::Pin<Box<dyn Future<Output = ()>>>>;

/// Here's version 1 subscription indexer relaying client.
/// It manage subscription with a M to M mapping. each new subscription **from a single Dapp connection**
/// will go with a new connection to backend full node.
pub struct MtoMSubClientPool {
    // full_node connecting uri.
    backend_uri: String,
    // all managed ws client, each ConnId use one connection with multiplexing subscription.
    clients: DashMap<ConnectionId, Arc<WsClient>>,
    // as a relay layer, we need to spawn a new thread with a single tiny tokio runtime
    // to run some initialization work, use this channel to send a task that will executed.
    // generally speaking, this tricky is to deal case: call a async fn in a sync fn in a async tokio runtime env.
    async_runner_sender: futures::channel::mpsc::Sender<OnceJob>,
}

impl MtoMSubClientPool {
    /// get a client by connId.
    fn get_client(&self, key: SubscriptionKey) -> anyhow::Result<Arc<Client>> {
        let client;
        if !self.clients.contains_key(&key.conn_id) {
            let uri = self.backend_uri.clone();
            let (tx, rx) = crossbeam::channel::bounded(1);
            self.async_runner_sender
                .clone()
                .try_send(Arc::new(move || {
                    let uri = uri.clone();
                    let tx = tx.clone();
                    let f = async move {
                        // todo, handle the situation while the client disconnects to full node.
                        let _ = match WsClientBuilder::default()
                            .connection_timeout(Duration::from_secs(60 * 60 * 24))
                            .build(uri)
                            .await
                        {
                            Ok(client) => tx.send(Ok(Arc::new(client))),
                            Err(e) => tx.send(Err(anyhow!("MtoM client creation fail: {:?}", e))),
                        };
                    };
                    Box::pin(f)
                }))?;
            client = rx.recv()??;
            self.clients.insert(key.conn_id, client.clone());
            trace!(
                "MtoM get a new connection with key: {:?}, now size: {}",
                key,
                self.clients.len()
            )
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
    pub fn subscribe<N>(
        &self,
        mut sink: SubscriptionSink,
        sub_method: &'static str,
        params: Option<ParamsSer<'static>>,
        unsub_method: &'static str,
    ) -> anyhow::Result<()>
    where
        N: for<'de> Deserialize<'de> + Serialize + Send + 'static,
    {
        // we need firstly accept the subscription from upstream, then we can get the sub_keys.
        let _ = sink.accept();
        let key = sink.sub_keys().ok_or_else(|| {
            error!(
                "{} get a subscription error, call before accept",
                sink.method_name()
            );
            anyhow!("")
        })?;
        let client = self.get_client(key)?;
        let (tx, rx) = crossbeam::channel::bounded(1);
        self.async_runner_sender
            .clone()
            .try_send(Arc::new(move || {
                let client = client.clone();
                let tx = tx.clone();
                let params = params.clone();
                let f = async move {
                    let _ = match client
                        .subscribe::<N>(sub_method, params, unsub_method)
                        .await
                    {
                        Ok(sub) => tx.send(Ok(sub)),
                        Err(e) => tx.send(Err(anyhow!("new client subscribe fail: {:?}", e))),
                    };
                };
                Box::pin(f)
            }))?;

        let sub_channel = rx.recv()??;
        // here means downstream subscription has been built successfully.
        let sub_channel = sub_channel.filter_map(|msg| async { msg.ok() });
        let sub_channel = Box::pin(sub_channel);

        tokio::spawn(async move {
            match sink.pipe_from_stream(sub_channel).await {
                SubscriptionClosed::Success => {
                    let err_obj: ErrorObjectOwned = SubscriptionClosed::Success.into();
                    sink.close(err_obj);
                }
                SubscriptionClosed::RemotePeerAborted => {}
                SubscriptionClosed::Failed(e) => {
                    sink.close(e);
                }
            }
        });
        Ok(())
    }

    pub fn new(backend_uri: String) -> Self {
        let (tx, mut rx) = futures::channel::mpsc::channel::<OnceJob>(64);
        // start a new inner work thread dealing with some closure job.
        std::thread::Builder::new()
            .name("MtoM_initial_worker".to_string())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                runtime.block_on(async move {
                    while let Some(job) = rx.next().await {
                        job().await;
                    }
                });
            })
            .unwrap();
        Self {
            backend_uri,
            clients: Default::default(),
            async_runner_sender: tx,
        }
    }
}
