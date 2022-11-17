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

pub use manta_indexer::errors::*;

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() -> Result<()> {
    // initialize logger
    // utils::init_logger();
    log4rs::init_file("conf/log.yaml", Default::default())?;

    let _handler = manta_indexer::service::start_service().await?;
    info!(target: "indexer", "indexer has been started successfully!");
    // todo, shutdown server gracefully.
    // if ctr + c {
    //     handler.stop().await?;
    // }
    futures::future::pending().await
}
