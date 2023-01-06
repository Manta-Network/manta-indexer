// Copyright 2020-2023 Manta Network.
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
use clap::Parser;
use frame_support::log::info;
use signal_hook::consts::{SIGINT, SIGQUIT, SIGTERM};
use std::sync::atomic::Ordering::SeqCst;
use std::time::Duration;

use manta_indexer::cli::IndexerCli;
pub use manta_indexer::errors::*;
use manta_indexer::utils::SHUTDOWN_FLAG;

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() -> Result<()> {
    let args = IndexerCli::parse();
    let config_path = format!("{}/log.yaml", args.config_path);
    let migrations_path = args.migrations_path;

    // initialize logger
    log4rs::init_file(config_path, Default::default())?;

    // used for graceful shutdown, each async task should hold a sender,
    // and drop the sender when it receive a shutdown signal and finish its loop.
    // then receiver will return a error means all sender are dropped, which means
    // all async task exit successfully, then we exit the main process.
    let (s, mut r) = tokio::sync::mpsc::channel::<()>(1);
    signal_hook::flag::register(SIGINT, SHUTDOWN_FLAG.clone())?;
    signal_hook::flag::register(SIGTERM, SHUTDOWN_FLAG.clone())?;
    signal_hook::flag::register(SIGQUIT, SHUTDOWN_FLAG.clone())?;

    let handler = manta_indexer::service::start_service(s, &migrations_path).await?;
    info!(target: "indexer", "indexer has been started successfully!");

    while !SHUTDOWN_FLAG.load(SeqCst) {
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    handler.stop()?.await;
    let _ = r.recv().await;
    info!(target: "indexer", "indexer has receive ctrl+c and finish graceful exit, bye");
    Ok(())
}
