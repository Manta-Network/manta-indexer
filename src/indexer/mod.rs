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

use crate::types::{DensePullResponse, PullResponse};
use jsonrpsee::{
    core::{async_trait, error::Error as JsonRpseeError, RpcResult},
    proc_macros::rpc,
};
use manta_pay::signer::Checkpoint;
use sqlx::sqlite::SqlitePool;

pub mod pull;
pub mod sync;

pub const MAX_SENDERS: u64 = 1024 * 16;
pub const MAX_RECEIVERS: u64 = 1024 * 16;

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
        mut max_receivers: u64,
        mut max_senders: u64,
    ) -> RpcResult<PullResponse> {
        // Currently, there's a limit on max size of reposne body, 10MB.
        // So 10MB means the params for (max_receivers, max_senders) is (1024 * 16, 1024 * 16).
        // If the params exceeds the value, pull_ledger_diff still returns 1024 * 16 utxos at most in one time.
        // so no error will be returned.
        if max_receivers > MAX_RECEIVERS || max_senders > MAX_SENDERS {
            max_receivers = MAX_RECEIVERS;
            max_senders = MAX_SENDERS;
        }

        let response =
            pull::pull_ledger_diff(&self.db_pool, &checkpoint, max_receivers, max_senders)
                .await
                .map_err(|e| JsonRpseeError::Custom(e.to_string()))?;

        Ok(response)
    }

    async fn densely_pull_ledger_diff(
        &self,
        checkpoint: Checkpoint,
        max_receivers: u64,
        max_senders: u64,
    ) -> RpcResult<DensePullResponse> {
        let raw = self
            .pull_ledger_diff(checkpoint, max_receivers, max_senders)
            .await?;
        Ok(raw.into())
    }
}

impl MantaPayIndexerServer {
    pub fn new(db_pool: SqlitePool) -> Self {
        Self { db_pool }
    }
}
