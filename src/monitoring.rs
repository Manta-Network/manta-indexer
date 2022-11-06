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

use frame_support::log::{debug, error, trace};
use jsonrpsee::core::middleware::{Headers, MethodKind, WsMiddleware};
use jsonrpsee::types::Params;
use once_cell::sync::Lazy;
use prometheus::{opts, register_int_counter_vec, IntCounterVec, Opts};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub const MANTA_INDEXER_MONITORING_NAMESPACE: &'static str = "manta_indexer";

/// this function helps wrapper common namespace and subsystems into a opts
/// with "manta_indexer" + "relay"
pub fn indexer_relay_opts(name: &str, help: &str) -> Opts {
    opts!(name, help)
        .namespace(MANTA_INDEXER_MONITORING_NAMESPACE)
        .subsystem("relay")
}

/// this function helps wrapper common namespace and subsystems into a opts
/// with "manta_indexer" + "ledger"
pub fn indexer_ledger_opts(name: &str, help: &str) -> Opts {
    opts!(name, help)
        .namespace(MANTA_INDEXER_MONITORING_NAMESPACE)
        .subsystem("ledger")
}

static TOTAL_RELAYING_COUNTER: Lazy<IntCounterVec> = Lazy::new(|| {
    let opts = indexer_relay_opts(
        "total_relay_count",
        "counting the total received relay request",
    );
    register_int_counter_vec!(opts, &["method", "status"]).expect("total_relay_counter alloc fail")
});

#[derive(Default, Clone)]
pub(crate) struct IndexerMiddleware {
    conn_num: Arc<AtomicU64>,
}

impl WsMiddleware for IndexerMiddleware {
    type Instant = Instant;

    fn on_connect(&self, remote_addr: SocketAddr, headers: &Headers) {
        let cn = self.conn_num.fetch_add(1, Ordering::SeqCst);
        debug!(
            target: "indexer",
            "[on_connect] remote_addr: {}, headers: {:?}, conn_num = {}",
            remote_addr,
            headers,
            cn + 1
        );
    }

    fn on_request(&self) -> Self::Instant {
        Instant::now()
    }

    // This function happens *before* the actual method calling.
    fn on_call(&self, method_name: &str, params: Params, kind: MethodKind) {
        match kind {
            MethodKind::Unknown => {
                error!(
                    target: "indexer",
                    "[on_call] receive a not existed method request: name = {}, params = {:?}",
                    method_name, params
                )
            }
            _ => {
                trace!(
                    target: "indexer",
                    "[on_call] receive a ws call: name = {}, params = {:?}, kind: {}",
                    method_name,
                    params,
                    kind
                )
            }
        }
    }

    // This function happens *after* the actual method calling.
    fn on_result(&self, name: &str, success: bool, started_at: Self::Instant) {
        trace!(
            target: "indexer",
            "[on_result] a ws call is finished, name = {}, success = {}, time = {:?} ms",
            name,
            success,
            started_at.elapsed().as_millis()
        );
    }

    // This function happens *before* the actual sending happens.
    // It's more like a starting signal of response.
    fn on_response(&self, result: &str, started_at: Self::Instant) {
        trace!(
            target: "indexer",
            "[on_response] a ws call is response, reply = {}, time = {:?} ms",
            result,
            started_at.elapsed().as_millis()
        );
    }

    fn on_disconnect(&self, remote_addr: SocketAddr) {
        let cn = self.conn_num.fetch_sub(1, Ordering::SeqCst);
        debug!(
            target: "indexer",
            "[on_disconnect] a remote_addr: {} disconnect, now conn = {}",
            remote_addr,
            cn - 1
        );
    }
}
