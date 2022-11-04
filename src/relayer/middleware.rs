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
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

#[derive(Default, Clone)]
pub(crate) struct IndexerMiddleware {
    conn_num: Arc<AtomicU64>,
}

impl WsMiddleware for IndexerMiddleware {
    type Instant = Instant;

    fn on_connect(&self, remote_addr: SocketAddr, headers: &Headers) {
        let cn = self.conn_num.fetch_add(1, Ordering::SeqCst);
        debug!(
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
                    "[on_call] receive a not existed method request: name = {}, params = {:?}",
                    method_name, params
                )
            }
            _ => {
                trace!(
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
            "[on_response] a ws call is response, reply = {}, time = {:?} ms",
            result,
            started_at.elapsed().as_millis()
        );
    }

    fn on_disconnect(&self, remote_addr: SocketAddr) {
        let cn = self.conn_num.fetch_sub(1, Ordering::SeqCst);
        debug!(
            "[on_disconnect] a remote_addr: {} disconnect, now conn = {}",
            remote_addr,
            cn - 1
        );
    }
}
