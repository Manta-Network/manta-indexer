/// Here's a helpful tools to help download the whole manta private ledger and insert into a local sqlite storage.
/// In some test stage, this tool helps a lot.
/// Visit configs in the beginning of `main` to get more knowledge.
use anyhow::Result;
use frame_support::log::LevelFilter;
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Logger, Root};
use log4rs::encode::pattern::PatternEncoder;
use log4rs::Config;
use manta_indexer::utils;

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
    let full_node = "ws://127.0.0.1:9800";
    // db path to save
    let sync_db = "local_sync.db";
    let pool_size = 16u32;
    let pool = manta_indexer::db::initialize_db_pool(sync_db, pool_size).await?;
    let ws_client = utils::create_ws_client(full_node).await?;
    manta_indexer::indexer::sync::sync_shards_from_full_node(
        &ws_client,
        &pool,
        (1024 * 4, 1024 * 4),
        false,
    )
    .await?;
    pool.close().await;
    Ok(())
}
