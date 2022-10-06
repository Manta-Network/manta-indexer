/// Relaying part relays the requests from Dapp to backend full node.
/// It plays a role as the startup part of service, which means we should raise up a
/// service use `start_server()` below and inject any udf logic into the server impl.
///
/// It parse the incoming requests and build a middle management mapping layer to manage the
/// connection from indexer to full node.
///
/// The ideal case is that relay server maintains N ws connections with full node but it holds
/// M ws connections from Dapp anywhere (M >> N).
pub mod relay_server;
pub mod middleware;

use std::net::SocketAddr;
use anyhow::{bail, Result};
use jsonrpsee::ws_server::WsServerHandle;
use std::time::Duration;
use jsonrpsee::core::TEN_MB_SIZE_BYTES;


const WS_DEFAULT_MAX_CONN: u64 = 1000;
const WS_DEFAULT_MAX_SUB_PER_CONN: u32 = 100;

/// JSON-RPC Websocket server settings.
/// Copied from jsonrpsee cause it's a private type.
#[derive(Debug, Clone)]
pub(crate) struct WsServerConfig {
    /// Maximum size in bytes of a request.
    max_request_body_size: u32,
    /// Maximum size in bytes of a response.
    max_response_body_size: u32,
    /// Maximum number of incoming connections allowed.
    max_connections: u64,
    /// Maximum number of subscriptions per connection.
    max_subscriptions_per_connection: u32,
    /// Max length for logging for requests and responses
    ///
    /// Logs bigger than this limit will be truncated.
    max_log_length: u32,
    /// Whether batch requests are supported by this server or not.
    batch_requests_supported: bool,
    /// The interval at which `Ping` frames are submitted.
    ping_interval: Duration,
}

impl Default for WsServerConfig {
    fn default() -> Self {
        Self {
            max_request_body_size: TEN_MB_SIZE_BYTES,
            max_response_body_size: TEN_MB_SIZE_BYTES,
            max_log_length: 4096,
            max_subscriptions_per_connection: WS_DEFAULT_MAX_SUB_PER_CONN,
            max_connections: WS_DEFAULT_MAX_CONN,
            batch_requests_supported: true,
            ping_interval: Duration::from_secs(60),
        }
    }
}


/// Here's start a relaying server
pub(crate) async fn start_server(config: Option<WsServerConfig>) -> Result<(SocketAddr, WsServerHandle)> {
    bail!("")
}