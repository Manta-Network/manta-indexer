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

#![allow(dead_code)]

use anyhow::Result;
use frame_support::log::info;
use prometheus::{Encoder, TextEncoder};
use std::time::Duration;
use tokio::time::sleep;

mod constants;
mod db;
mod errors;
mod indexer;
mod logger;
mod monitoring;
mod relayer;
mod service;
mod types;
mod utils;

pub use errors::*;

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() -> Result<()> {
    // initialize logger
    // utils::init_logger();
    log4rs::init_file("conf/log.yaml", Default::default())?;

    let _handler = service::start_service().await?;
    // todo, shutdown server gracefully.
    // if ctr + c {
    //     handler.stop().await?;
    // }
    loop {
        sleep(Duration::from_secs(2)).await;
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        let m = prometheus::gather();
        encoder.encode(&m, &mut buffer).unwrap();
        info!(target: "indexer", "prometheus info: {}", String::from_utf8(buffer).unwrap());
    }
    futures::future::pending().await
}
