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

mod constants;
mod db;
mod errors;
mod ledger_sync;
mod logger;
mod relay;
mod types;
mod utils;

pub use errors::*;

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() -> Result<()> {
    // initialize logger
    utils::init_logger();

    // // relay::start_relayer_server().await;
    // let _ = crate::ledger_sync::MantaPayIndexerServer::start_server().await;
    futures::future::pending().await
}
