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
use crate::types::{EncryptedNote, PullResponse, ReceiverChunk, SenderChunk, Utxo};
use anyhow::Result;
use codec::Decode;
use manta_pay::signer::Checkpoint;
use sqlx::sqlite::SqlitePool;
use std::collections::HashMap;

/// Calculate the next checkpoint.
pub fn calculate_next_checkpoint(
    previous_shards: &HashMap<u8, Vec<(Utxo, EncryptedNote)>>,
    previous_checkpoint: &Checkpoint,
    next_checkpoint: &mut Checkpoint,
    sender_index: usize,
) {
    // point to next void number
    next_checkpoint.sender_index += sender_index;
    for (i, utxos) in previous_shards.iter() {
        let index = *i as usize;
        // offset for next shard
        next_checkpoint.receiver_index[index] =
            previous_checkpoint.receiver_index[index] + utxos.len()
    }
}

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

pub async fn pull_receivers_for_shard(
    pool: &SqlitePool,
    shard_index: u8,
    receiver_index: usize,
    max_update: u64,
    receivers: &mut ReceiverChunk,
    receivers_pulled: &mut u64,
) -> Result<bool> {
    let max_receiver_index = (receiver_index as u64) + max_update;
    let shards =
        crate::db::get_batched_shards(pool, shard_index, receiver_index as u64, max_receiver_index)
            .await?;

    let mut idx = receiver_index;
    for shard in shards {
        if *receivers_pulled == max_update {
            return Ok(crate::db::has_shard(pool, shard_index, idx as u64).await);
        }
        *receivers_pulled += 1;
        let mut utxo = shard.utxo.as_slice();
        let n: (Utxo, EncryptedNote) = <(Utxo, EncryptedNote) as Decode>::decode(&mut utxo)?;
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
    let mut senders = Vec::new();
    let max_sender_index = if max_update_request > PULL_MAX_SENDER_UPDATE_SIZE {
        (sender_index as u64) + PULL_MAX_SENDER_UPDATE_SIZE
    } else {
        (sender_index as u64) + max_update_request
    };
    // let batched_vns = crate::db::get_batched_void_number(pool, sender_index, max_sender_index - 1).await?;
    // todo, batch reads instead of read vn one by one.
    for idx in (sender_index as u64)..max_sender_index {
        match crate::db::get_one_void_number(pool, idx).await {
            Ok(next) => senders.push(next),
            _ => return Ok((false, senders)),
        }
    }
    Ok((
        crate::db::has_void_number(pool, max_sender_index as u64).await,
        senders,
    ))
}

pub async fn pull_ledger_diff(
    pool: &SqlitePool,
    checkpoint: &Checkpoint,
    max_receivers: u64,
    max_senders: u64,
) -> Result<PullResponse> {
    let (more_receivers, receivers) =
        pull_receivers(pool, *checkpoint.receiver_index, max_receivers).await?;
    let (more_senders, senders) = pull_senders(pool, checkpoint.sender_index, max_senders).await?;
    // let senders_receivers_total = sqlx::query!(
    //     "SELECT total FROM senders_receivers_total;",
    // )
    // .fetch_one(pool)
    // .await?;
    let senders_receivers_total = 0;

    Ok(PullResponse {
        should_continue: more_receivers,
        receivers,
        senders,
        senders_receivers_total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
}
