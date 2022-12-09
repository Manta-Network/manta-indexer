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

use crate::types::{Checkpoint, FullIncomingNote, PullResponse, ReceiverChunk, SenderChunk, Utxo};
use anyhow::Result;
use codec::Decode;
use frame_support::log::{debug, trace};
use manta_pay::config::utxo::MerkleTreeConfiguration;
use sqlx::sqlite::SqlitePool;
use std::collections::HashMap;
use tokio_stream::StreamExt;

/// Calculate the next checkpoint.
pub async fn calculate_next_checkpoint(
    previous_shards: &HashMap<u8, ReceiverChunk>,
    previous_checkpoint: &Checkpoint,
    next_checkpoint: &mut Checkpoint,
    sender_index: usize,
) {
    // point to next nullifier commitment
    next_checkpoint.sender_index += sender_index;
    let mut stream_shards = tokio_stream::iter(previous_shards.iter());
    while let Some((i, utxos)) = stream_shards.next().await {
        let index = *i as usize;
        // offset for next shard
        next_checkpoint.receiver_index[index] =
            previous_checkpoint.receiver_index[index] + utxos.len()
    }
}

pub(crate) type ReceiverIndexArray = [usize; MerkleTreeConfiguration::FOREST_WIDTH];
/// pull receivers from local sqlite with given checkpoint indices position.
/// Args:
///     * `max_amount`: max batch size of this request.
/// Return:
///     more_receivers[bool]: whether exists more receivers.
///     index[ReceiverIndexArray]: if more_receivers, next request index. if not, data doesn't make sense.
///     receiver_chunk: content.
pub async fn pull_receivers(
    pool: &SqlitePool,
    receiver_indices: ReceiverIndexArray,
    max_amount: u64,
) -> Result<(bool, ReceiverIndexArray, ReceiverChunk)> {
    let mut more_receivers = false;
    let mut receivers = Vec::new();
    let mut next_indices = receiver_indices;
    let mut remain = max_amount;

    for (shard_index, utxo_index) in receiver_indices.into_iter().enumerate() {
        debug!(
            "query shard index: {}, and utxo_index: {}",
            shard_index, utxo_index
        );
        let before_pull = receivers.len();
        more_receivers |=
            pull_receivers_for_shard(pool, shard_index as u8, utxo_index, remain, &mut receivers)
                .await?;
        let after_pull = receivers.len();
        next_indices[shard_index] = utxo_index + after_pull - before_pull;
        if receivers.len() == max_amount as usize {
            break;
        }
        remain = max_amount - receivers.len() as u64;
    }
    Ok((more_receivers, next_indices, receivers))
}

/// pull a specific shard of receivers from local sqlite.
/// Args:
///     * `receiver_index`: the beginning from this `shard index`
///     * `amount`: the amount wanted in this shard, index from `receiver_index`.
///     * `receivers`: mutable chunk to append new receive data in
///     * `receivers_pulled`: total pulled counter.
/// Returns:
///     * A bool value to indicate whether more receiver items existed in this shard.
pub async fn pull_receivers_for_shard(
    pool: &SqlitePool,
    shard_index: u8,
    receiver_index: usize,
    amount: u64,
    receivers: &mut ReceiverChunk,
) -> Result<bool> {
    let max_receiver_index = (receiver_index as u64) + amount;
    trace!(
        target: "indexer",
        "query shard index: {}, and receiver_index: {}, {}",
        shard_index, receiver_index, max_receiver_index
    );
    let shards =
        crate::db::get_batched_shards(pool, shard_index, receiver_index as u64, max_receiver_index)
            .await?;

    let mut stream_shards = tokio_stream::iter(shards.iter());
    while let Some(shard) = stream_shards.next().await {
        let mut utxo = shard.utxo.as_slice();
        let mut full_incoming_note = shard.full_incoming_note.as_slice();
        receivers.push((
            <Utxo as Decode>::decode(&mut utxo)?,
            <FullIncomingNote as Decode>::decode(&mut full_incoming_note)?,
        ));
    }

    // TODO if some hole exists in utxo_index, this logic gonna be error.
    Ok(shards.len() == amount as usize
        && crate::db::has_item(pool, shard_index, max_receiver_index).await)
}

/// pull senders from local sqlite with given checkpoint indices position.
/// Return:
///     more_senders[bool]: whether exists more receivers.
///     index[u64]: if more_senders, next request index. if not, data doesn't make sense.
///     sender_chunk: content.
pub async fn pull_senders(
    pool: &SqlitePool,
    sender_index: u64,
    max_update: u64,
) -> Result<(bool, u64, SenderChunk)> {
    Ok((
        crate::db::has_nullifier(pool, sender_index as u64 + max_update).await,
        sender_index + max_update,
        crate::db::get_batched_nullifier(
            pool,
            sender_index as u64,
            (sender_index + max_update) as u64,
        )
        .await?,
    ))
}

/// pull_ledger_diff from local sqlite from given checkpoint position.
/// Result:
///     * A formatted PullResponse result + next Checkpoint cursor.
///     * next cursor works only if pull_response.should_continue = true.
pub async fn pull_ledger_diff(
    pool: &SqlitePool,
    checkpoint: &Checkpoint,
    max_receivers: u64,
    max_senders: u64,
) -> Result<(PullResponse, Checkpoint)> {
    trace!(target: "indexer", "receive a pull_ledger_diff req with max_sender: {}, max_receiver: {}", max_receivers, max_senders);

    let (more_receivers, next_receiver_index, receivers) =
        pull_receivers(pool, *checkpoint.receiver_index, max_receivers).await?;
    let (more_senders, next_sender_index, senders) =
        pull_senders(pool, checkpoint.sender_index as u64, max_senders).await?;
    let senders_receivers_total = crate::db::get_total_senders_receivers(pool).await?;
    Ok((
        PullResponse {
            should_continue: more_receivers || more_senders,
            receivers,
            senders,
            senders_receivers_total,
        },
        Checkpoint {
            receiver_index: next_receiver_index.into(),
            sender_index: next_sender_index as usize,
        },
    ))
}
