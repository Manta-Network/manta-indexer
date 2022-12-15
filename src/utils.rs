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

use anyhow::Result;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use std::{
    fs::File,
    io::{prelude::*, BufReader},
};
use tokio::time::{sleep, Duration};
use toml::Value;

// read project config file
pub fn read_config() -> Result<Value> {
    let config = File::open(concat!(env!("CARGO_MANIFEST_DIR"), "/conf/config.toml"))?;
    let mut buff = BufReader::new(config);
    let mut contents = String::new();
    buff.read_to_string(&mut contents)?;

    let value = contents.parse::<Value>()?;
    Ok(value)
}

pub async fn create_ws_client(url: &str) -> Result<WsClient> {
    let client = WsClientBuilder::default()
        .connection_timeout(Duration::from_secs(3))
        .request_timeout(Duration::from_secs(3))
        .ping_interval(Duration::from_secs(15))
        .build(&url)
        .await?;

    Ok(client)
}

pub async fn is_full_node(ws: &WsClient) -> bool {
    let roles = ws
        .request::<Vec<String>>("system_nodeRoles", None)
        .await
        .expect("Unknown roles.");
    roles.iter().any(|r| r == "Full")
}

pub async fn is_the_rpc_methods_existed(ws: &WsClient, rpc_method: &str) -> Result<bool> {
    use serde::{Deserialize, Serialize};
    #[derive(Clone, Debug, Deserialize, Serialize)]
    pub struct RpcMethods {
        version: u32,
        methods: Vec<String>,
    }

    let methods = ws.request::<RpcMethods>("rpc_methods", None).await?;
    Ok(methods.methods.iter().any(|m| m == rpc_method))
}

pub fn build_tokio_runtime() -> Result<tokio::runtime::Runtime> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .on_thread_start(|| {
            println!("manta indexer servier is started!");
        })
        .on_thread_stop(|| {
            println!("manta indexer servier is stoped!");
        })
        .enable_all()
        .build()?;

    Ok(rt)
}

// Ensure full node is started first.
pub async fn is_full_node_started(ws: &WsClient) -> Result<bool> {
    let current_block = ws
        .request::<String>("chain_getBlockHash", rpc_params![])
        .await?;

    // sleep 12-13 seconds to see whether any new block is produced.
    sleep(Duration::from_secs(13)).await;
    let latest_block = ws
        .request::<String>("chain_getBlockHash", rpc_params![])
        .await?;

    Ok(latest_block != current_block)
}

use once_cell::sync::{Lazy, OnceCell};
use std::ops::Deref;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// OnceStatic is a wrapper for static object initializing with OnceCell Lazy.
/// It allows an active initialization during the system startup phase.
/// # Example
/// ```ignore
/// // before we have OnceStatic, we can only initial one variable with a closure function.
/// // but sometime we need to firstly read some configuration and initial them by that.
/// static x: Config = Lazy::new(|| Default::default());
/// // Using OnceStatic, like:
/// static x: OnceStatic<Config> = OnceStatic::new("x");
/// x.init(|| Default::default());
/// println!("{}, {}", x.a, x.b)
/// ```
#[derive(Debug)]
pub struct OnceStatic<T> {
    inner: OnceCell<T>,
    name: &'static str,
}

impl<T> OnceStatic<T> {
    pub fn init<F: FnOnce() -> Result<T>>(&self, f: F) -> Result<()> {
        match self.inner.set(f()?) {
            Ok(_) => Ok(()),
            Err(_) => unreachable!("global config {} init twice", self.name),
        }
    }

    /// * `name`: It's a name for debugging. we can optimize the code style with macro soon.
    ///  for now, just type it same with variable name.
    /// Eg: static GLOBAL_V: OnceStatic<Config> = OnceStatic::new("GLOBAL_V");
    pub const fn new(name: &'static str) -> OnceStatic<T> {
        OnceStatic {
            inner: OnceCell::new(),
            name,
        }
    }
}

impl<T> Deref for OnceStatic<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.inner
            .get()
            .unwrap_or_else(|| panic!("static wrapper {} not init yet.", self.name))
    }
}

/// A global graceful shutdown switch, will be set true when receive a ctrl+c.
/// Each async task should watch this and deal with shutdown.
pub static SHUTDOWN_FLAG: Lazy<Arc<AtomicBool>> = Lazy::new(|| Arc::new(AtomicBool::new(false)));

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_node_role_should_work() {
        let config = read_config().unwrap();
        let node = config["indexer"]["configuration"]["full_node"]
            .as_str()
            .unwrap();
        let ws = create_ws_client(node)
            .await
            .expect("failed to create ws client.");
        // the node started in CI is a collator.
        assert!(!is_full_node(&ws).await);
    }

    #[tokio::test]
    async fn pull_rpc_method_must_exist_in_full_node() {
        let config = read_config().unwrap();
        let node = config["indexer"]["configuration"]["full_node"]
            .as_str()
            .unwrap();
        let rpc_method = config["indexer"]["configuration"]["rpc_method"]
            .as_str()
            .unwrap();
        let ws = create_ws_client(node)
            .await
            .expect("failed to create ws client.");
        assert!(is_the_rpc_methods_existed(&ws, rpc_method).await.unwrap());
    }

    #[tokio::test]
    async fn full_node_must_be_started_first() {
        let config = read_config().unwrap();
        let node = config["indexer"]["configuration"]["full_node"]
            .as_str()
            .unwrap();
        let ws = create_ws_client(node)
            .await
            .expect("failed to create ws client.");
        let is_started = is_full_node_started(&ws).await;
        assert!(is_started.is_ok());
        assert!(is_started.unwrap());
    }
}
