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

/// Here's a helpful tools to help download the whole manta private ledger and insert into a local sqlite storage.
/// In some test stage, this tool helps a lot.
/// Visit configs in the beginning of `main` to get more knowledge.
use anyhow::Result;
use frame_support::log::LevelFilter;
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Logger, Root};
use log4rs::encode::pattern::PatternEncoder;
use log4rs::Config;

#[tokio::main]
async fn main() -> Result<()> {
    // set log into stdout
    let log4rs_config = Config::builder()
        .appender(
            Appender::builder().build(
                "stdout",
                Box::new(
                    ConsoleAppender::builder()
                        .encoder(Box::new(PatternEncoder::new(
                            "{d(%Y-%m-%d %H:%M:%S.%3f)} {l} {f}:{L} - {m}{n}",
                        )))
                        .build(),
                ),
            ),
        )
        .logger(
            Logger::builder()
                .appenders(["stdout"])
                .build("indexer", LevelFilter::Debug),
        )
        .build(Root::builder().build(LevelFilter::Warn))?;
    log4rs::init_config(log4rs_config)?;

    // full node url to fetch ledgers.
    let full_node = "wss://ws.rococo.dolphin.engineering:443";
    // db path to save
    let sync_db = "local_sync.db";
    let pool_size = 16u32;
    let pool = manta_indexer::db::initialize_db_pool(sync_db, pool_size).await?;
    manta_indexer::indexer::sync::pull_all_shards_to_db(&pool, full_node).await?;
    pool.close().await;
    Ok(())
}
