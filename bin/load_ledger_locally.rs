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

    // this loop args means we repeat the pull, the reason we need it
    // is that in some testnet, the data size may be too small to do some stress test.
    // so we clone the data by times to simulate the future scenario.
    // if you don't care about this case, just set loop_time = 1;
    let loop_time = 100;
    for i in 0..loop_time {
        let _r = manta_indexer::indexer::sync::pull_all_shards_to_db(&pool, full_node).await?;
    }
    pool.close().await;
    Ok(())
}
