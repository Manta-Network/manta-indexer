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

use codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::mem;

pub use pallet_manta_pay::types::{
    FullIncomingNote, NullifierCommitment, OutgoingNote, PullResponse, ReceiverChunk, SenderChunk,
    Utxo,
};

pub const CIPHER_TEXT_LENGTH: usize = 68;
pub const EPHEMERAL_PUBLIC_KEY_LENGTH: usize = 32;
pub const UTXO_LENGTH: usize = 32;
pub const VOID_NUMBER_LENGTH: usize = 32;

pub const fn size_of_utxo() -> usize {
    UTXO_LENGTH
}

pub const fn size_of_encrypted_note() -> usize {
    EPHEMERAL_PUBLIC_KEY_LENGTH + CIPHER_TEXT_LENGTH
}

pub const fn size_of_void_number() -> usize {
    VOID_NUMBER_LENGTH
}

pub fn size_of_receiver_chunk(chunk: &ReceiverChunk) -> usize {
    chunk.len() * (size_of_encrypted_note() + size_of_encrypted_note())
}

pub fn size_of_sender_chunk(chunk: &SenderChunk) -> usize {
    chunk.len() * size_of_void_number()
}

pub fn size_of_pull_response(resp: &PullResponse) -> usize {
    let mut _size = 0;
    _size += mem::size_of_val(&resp.should_continue);
    _size += mem::size_of_val(&resp.senders_receivers_total);
    _size += size_of_sender_chunk(&resp.senders);
    _size += size_of_receiver_chunk(&resp.receivers);

    _size
}

pub fn upper_bound_for_response(payload_size: usize) -> (usize, usize) {
    let senders_share = size_of_void_number() as f32
        / (size_of_encrypted_note() + size_of_encrypted_note() + size_of_void_number()) as f32;
    let receivers_share = 1.0f32 - senders_share;

    let sender_len = senders_share * payload_size as f32 / size_of_void_number() as f32;
    let receivers_len = receivers_share * payload_size as f32
        / (size_of_encrypted_note() + size_of_encrypted_note()) as f32;
    (sender_len as usize, receivers_len as usize)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RpcMethods {
    version: u32,
    methods: Vec<String>,
}

/// Health struct returned by the RPC
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    /// Number of connected peers
    pub peers: usize,
    /// Is the node syncing
    pub is_syncing: bool,
    /// Should this node have any peers
    ///
    /// Might be false for local chains or when running without discovery.
    pub should_have_peers: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Shard {
    pub shard_index: u8,
    pub next_index: i64,
    pub utxo: Vec<u8>,
}
