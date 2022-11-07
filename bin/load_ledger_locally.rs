/// Here's a helpful tools to help download the whole manta private ledger and insert into a local sqlite storage.
/// In some test stage, this tool helps a lot.
/// Visit configs in the beginning of `main` to get more knowledge.
use anyhow::Result;
use std::sync::Arc;
#[tokio::main]
async fn main() -> Result<()> {
    // full node url to fetch ledgers.
    let full_node = "wss://ws.rococo.dolphin.engineering:443";
    // db path to save
    let sync_db = "local_sync.db";
    let pool_size = 16u32;
    let pool = manta_indexer::db::initialize_db_pool(sync_db, pool_size).await?;
    let r = manta_indexer::indexer::sync::pull_all_shards_to_db(&pool, full_node).await?;
    Ok(())
}
