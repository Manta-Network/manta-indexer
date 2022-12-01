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

use crate::indexer::sync::reconstruct_shards_from_pull_response;
use crate::types::{Checkpoint, DensePullResponse, PullResponse};
use codec::Encode;
use jsonrpsee::{
    core::{async_trait, error::Error as JsonRpseeError, RpcResult},
    proc_macros::rpc,
};
use sqlx::sqlite::SqlitePool;

pub mod pull;
pub mod sync;

pub const MAX_SENDERS: u64 = 1024 * 4;
pub const MAX_RECEIVERS: u64 = 1024 * 4;

#[rpc(server, namespace = "mantaPay")]
pub trait MantaPayIndexerApi {
    #[method(name = "pull_ledger_diff")] // no blocking mode, we just query all shards from db
    async fn pull_ledger_diff(
        &self,
        checkpoint: Checkpoint,
        max_receivers: u64,
        max_senders: u64,
    ) -> RpcResult<PullResponse>;

    /// Same semantic of `pull_ledger_diff`, but return a dense response,
    /// which is more friendly for transmission performance.
    #[method(name = "densely_pull_ledger_diff")]
    async fn densely_pull_ledger_diff(
        &self,
        checkpoint: Checkpoint,
        max_receivers: u64,
        max_senders: u64,
    ) -> RpcResult<DensePullResponse>;
}

pub struct MantaPayIndexerServer {
    // db pool
    pub db_pool: SqlitePool,
}

#[async_trait]
impl MantaPayIndexerApiServer for MantaPayIndexerServer {
    async fn pull_ledger_diff(
        &self,
        checkpoint: Checkpoint,
        max_receivers: u64,
        max_senders: u64,
    ) -> RpcResult<PullResponse> {
        let response = self
            .pull_ledger_diff_impl(checkpoint, max_receivers, max_senders)
            .await?;
        Ok(response.0)
    }

    async fn densely_pull_ledger_diff(
        &self,
        checkpoint: Checkpoint,
        max_receivers: u64,
        max_senders: u64,
    ) -> RpcResult<DensePullResponse> {
        let (raw, next_checkpoint) = self
            .pull_ledger_diff_impl(checkpoint, max_receivers, max_senders)
            .await?;
        Ok(DensePullResponse {
            sender_receivers_total: raw.senders_receivers_total,
            receiver_len: raw.receivers.len(),
            receivers: hex::encode(raw.receivers.encode()),
            sender_len: raw.senders.len(),
            senders: hex::encode(raw.senders.encode()),
            should_continue: raw.should_continue,
            next_checkpoint,
        })
    }
}

impl MantaPayIndexerServer {
    pub fn new(db_pool: SqlitePool) -> Self {
        Self { db_pool }
    }

    async fn pull_ledger_diff_impl(
        &self,
        checkpoint: Checkpoint,
        mut max_receivers: u64,
        mut max_senders: u64,
    ) -> RpcResult<(PullResponse, Checkpoint)> {
        // Currently, there's a limit on max size of response body, 10MB.
        // We need to limit the max amount according to ReceiverChunk and SenderChunk.
        // If the params exceeds the value, pull_ledger_diff still returns limitation data at most in one time.
        // so no error will be returned.
        max_receivers = max_receivers.min(MAX_RECEIVERS);
        max_senders = max_senders.min(MAX_SENDERS);
        Ok(
            pull::pull_ledger_diff(&self.db_pool, &checkpoint, max_receivers, max_senders)
                .await
                .map_err(|e| JsonRpseeError::Custom(e.to_string()))?,
        )
    }
}
