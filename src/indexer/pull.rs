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

use crate::db::has_shard;
use crate::indexer::cache::{
    get_batch_sender, get_batch_shard_receiver, put_batch_receiver, put_batch_sender,
};
use crate::types::{Checkpoint, FullIncomingNote, PullResponse, ReceiverChunk, SenderChunk, Utxo};
use anyhow::Result;
use codec::{Decode, Encode};
use frame_support::log::{debug, trace};
use manta_pay::config::utxo::MerkleTreeConfiguration;
use pallet_manta_pay::types::{NullifierCommitment, OutgoingNote};
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
        // Unlike sender query, receiver query has a tricky thing:
        // In a initialization looping, the front shard should be traversed first second request,
        // and never visited in this init loop, so for the following request,
        // we need to just skip invalid pull_for_shard for those shards.
        if !has_shard(pool, shard_index as u8, utxo_index as u64).await {
            continue;
        }

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
    let all_indices = (receiver_index..max_receiver_index as usize).collect::<Vec<usize>>();

    // step1. check cached hot data.
    let mut cached_values = 0;
    let cached = get_batch_shard_receiver(shard_index, &all_indices).await;
    let mut query_db_indices = vec![];
    for (offset, v) in cached.into_iter().enumerate() {
        if let Some(v) = v {
            receivers.push(<(Utxo, FullIncomingNote) as Decode>::decode(
                &mut v.as_slice(),
            )?);
            cached_values += 1;
        } else {
            query_db_indices.push(all_indices[offset] as i64)
        }
    }

    // step2. query db for cache missing data.
    let shards = crate::db::get_batched_shards_by_idxs(pool, shard_index, query_db_indices).await?;
    let mut write_back = Vec::with_capacity(shards.len());

    let mut stream_shards = tokio_stream::iter(shards.iter());
    while let Some(shard) = stream_shards.next().await {
        let mut utxo = shard.utxo.as_slice();
        let mut full_incoming_note = shard.full_incoming_note.as_slice();
        let receiver = (
            <Utxo as Decode>::decode(&mut utxo)?,
            <FullIncomingNote as Decode>::decode(&mut full_incoming_note)?,
        );
        write_back.push((shard.utxo_index as usize, receiver.encode()));
        receivers.push(receiver);
    }

    // step3. write back to cache.
    put_batch_receiver(shard_index, write_back).await;

    // TODO if some hole exists in utxo_index, this logic gonna be error.
    Ok(shards.len() + cached_values as usize == amount as usize
        && crate::db::has_shard(pool, shard_index, max_receiver_index).await)
}

/// pull senders from local sqlite with given checkpoint indices position.
/// Return:
///     more_senders[bool]: whether exists more receivers.
///     index[u64]: if more_senders, next request index. if not, data doesn't make sense.
///     sender_chunk: content.
pub async fn pull_senders(
    pool: &SqlitePool,
    sender_index: u64,
    amount: u64,
) -> Result<(bool, u64, SenderChunk)> {
    let mut senders = vec![];
    let all_indices = (sender_index..(sender_index + amount))
        .map(|v| v as usize)
        .collect::<Vec<usize>>();
    // step1. check cached hot data.
    let cached = get_batch_sender(&all_indices).await;
    let mut query_db_indices = vec![];
    for (offset, v) in cached.into_iter().enumerate() {
        if let Some(v) = v {
            senders.push(<(NullifierCommitment, OutgoingNote) as Decode>::decode(
                &mut v.as_slice(),
            )?);
        } else {
            query_db_indices.push(all_indices[offset] as i64)
        }
    }

    // step2. query db for cache missing data.
    let nullifiers = crate::db::get_batched_nullifier_by_idxs(pool, query_db_indices).await?;
    let mut write_back = Vec::with_capacity(nullifiers.len());

    let mut stream_shards = tokio_stream::iter(nullifiers.iter());
    while let Some(nullifier) = stream_shards.next().await {
        let mut nullifier_commitment = nullifier.nullifier_commitment.as_slice();
        let mut outgoing_note = nullifier.outgoing_note.as_slice();
        let sender = (
            <NullifierCommitment as Decode>::decode(&mut nullifier_commitment)?,
            <OutgoingNote as Decode>::decode(&mut outgoing_note)?,
        );
        write_back.push((nullifier.idx as usize, sender.encode()));
        senders.push(sender);
    }

    // step3. write back to cache.
    put_batch_sender(write_back).await;

    Ok((
        crate::db::has_nullifier(pool, sender_index + amount).await,
        sender_index + amount,
        senders,
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
