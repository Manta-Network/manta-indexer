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

use crate::constants::*;
use crate::types::{FullIncomingNote, PullResponse, ReceiverChunk, SenderChunk, Utxo};
use anyhow::Result;
use codec::Decode;
use frame_support::log::{debug, trace};
use manta_pay::config::utxo::v2::Checkpoint;
use sqlx::sqlite::SqlitePool;
use std::collections::HashMap;
use tokio_stream::StreamExt;

/// Calculate the next checkpoint.
pub async fn calculate_next_checkpoint(
    previous_shards: &HashMap<u8, Vec<(Utxo, FullIncomingNote)>>,
    previous_checkpoint: &Checkpoint,
    next_checkpoint: &mut Checkpoint,
    sender_index: usize,
) {
    // point to next void number
    next_checkpoint.sender_index += sender_index;
    let mut stream_shards = tokio_stream::iter(previous_shards.iter());
    while let Some((i, utxos)) = stream_shards.next().await {
        let index = *i as usize;
        // offset for next shard
        next_checkpoint.receiver_index[index] =
            previous_checkpoint.receiver_index[index] + utxos.len()
    }
}

/// pull receivers from local sqlite with given checkpoint indices position.
pub async fn pull_receivers(
    pool: &SqlitePool,
    receiver_indices: [usize; 256],
    max_update_request: u64,
) -> Result<(bool, ReceiverChunk)> {
    let mut more_receivers = false;
    let mut receivers = Vec::new();
    let mut receivers_pulled: u64 = 0;
    let max_update = if max_update_request > PULL_MAX_RECEIVER_UPDATE_SIZE {
        PULL_MAX_RECEIVER_UPDATE_SIZE
    } else {
        max_update_request
    };

    for (shard_index, utxo_index) in receiver_indices.into_iter().enumerate() {
        debug!(
            "query shard index: {}, and utxo_index: {}",
            shard_index, utxo_index
        );
        more_receivers |= pull_receivers_for_shard(
            pool,
            shard_index as u8,
            utxo_index,
            max_update,
            &mut receivers,
            &mut receivers_pulled,
        )
        .await?;
        if receivers_pulled == max_update && more_receivers {
            break;
        }
    }
    Ok((more_receivers, receivers))
}

/// pull a specific shard of receivers from local sqlite.
/// Args:
///     * `receiver_index`: the beginning from this `shard index`
///     * `receivers`: mutable chunk to append new receive data in
///     * `receivers_pulled`: total pulled counter.
pub async fn pull_receivers_for_shard(
    pool: &SqlitePool,
    shard_index: u8,
    receiver_index: usize,
    max_update: u64,
    receivers: &mut ReceiverChunk,
    receivers_pulled: &mut u64,
) -> Result<bool> {
    let max_receiver_index = (receiver_index as u64) + max_update;
    debug!(
        "query shard index: {}, and receiver_index: {}, {}",
        shard_index, receiver_index, max_receiver_index
    );
    let shards =
        crate::db::get_batched_shards(pool, shard_index, receiver_index as u64, max_receiver_index)
            .await?;

    let mut idx = receiver_index;
    let mut stream_shards = tokio_stream::iter(shards.iter());
    while let Some(shard) = stream_shards.next().await {
        if *receivers_pulled == max_update {
            return Ok(crate::db::has_shard(pool, shard_index, idx as u64).await);
        }
        *receivers_pulled += 1;
        let mut utxo = shard.utxo.as_slice();
        let n: (Utxo, FullIncomingNote) = <(Utxo, FullIncomingNote) as Decode>::decode(&mut utxo)?;
        receivers.push(n);

        idx += 1;
    }
    Ok(crate::db::has_shard(pool, shard_index, max_receiver_index).await)
}

pub async fn pull_senders(
    pool: &SqlitePool,
    sender_index: usize,
    max_update_request: u64,
) -> Result<(bool, SenderChunk)> {
    let max_sender_index = if max_update_request > PULL_MAX_SENDER_UPDATE_SIZE {
        (sender_index as u64) + PULL_MAX_SENDER_UPDATE_SIZE
    } else {
        (sender_index as u64) + max_update_request
    };

    let senders = if crate::db::has_nullifier(pool, max_sender_index - 1).await {
        crate::db::get_batched_nullifier(pool, sender_index as u64, max_sender_index - 1).await?
    } else {
        let length_of_vns = crate::db::get_len_of_nullifier(pool).await? as u64;
        let senders =
            crate::db::get_batched_nullifier(pool, sender_index as u64, length_of_vns - 1).await?;
        return Ok((false, senders));
    };

    Ok((
        crate::db::has_nullifier(pool, max_sender_index as u64).await,
        senders,
    ))
}

/// pull_ledger_diff from local sqlite from given checkpoint position.
pub async fn pull_ledger_diff(
    pool: &SqlitePool,
    checkpoint: &Checkpoint,
    max_receivers: u64,
    max_senders: u64,
) -> Result<PullResponse> {
    trace!(target: "indexer", "receive a pull_ledger_diff req with max_sender: {}, max_receiver: {}", max_receivers, max_senders);

    let (more_receivers, receivers) =
        pull_receivers(pool, *checkpoint.receiver_index, max_receivers).await?;
    let (more_senders, senders) = pull_senders(pool, checkpoint.sender_index, max_senders).await?;
    let senders_receivers_total = crate::db::get_total_senders_receivers(pool).await? as u128;

    Ok(PullResponse {
        should_continue: more_receivers || more_senders,
        receivers,
        senders,
        senders_receivers_total,
    })
}
