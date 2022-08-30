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

use jsonrpsee::core::middleware::{Headers, MethodKind, Params, WsMiddleware};
use std::net::SocketAddr;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct IndexerLogger;

impl WsMiddleware for IndexerLogger {
    type Instant = Instant;

    fn on_connect(&self, remote_addr: SocketAddr, headers: &Headers) {
        println!(
            "[IndexerLogger::on_connect] remote_addr {}, headers: {:?}",
            remote_addr, headers
        );
    }

    fn on_request(&self) -> Self::Instant {
        println!("[IndexerLogger::on_request]");
        Instant::now()
    }

    fn on_call(&self, name: &str, params: Params, kind: MethodKind) {
        println!(
            "[IndexerLogger::on_call] method: '{}', params: {:?}, kind: {}",
            name, params, kind
        );
    }

    fn on_result(&self, name: &str, succeess: bool, started_at: Self::Instant) {
        println!(
            "[IndexerLogger::on_result] '{}', worked? {}, time elapsed {:?}",
            name,
            succeess,
            started_at.elapsed()
        );
    }

    fn on_response(&self, result: &str, started_at: Self::Instant) {
        println!(
            "[IndexerLogger::on_response] result: {}, time elapsed {:?}",
            result,
            started_at.elapsed()
        );
    }

    fn on_disconnect(&self, remote_addr: SocketAddr) {
        println!(
            "[IndexerLogger::on_disconnect] remote_addr: {}",
            remote_addr
        );
    }
}
