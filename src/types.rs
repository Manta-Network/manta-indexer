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

use serde::{Deserialize, Serialize};

pub use pallet_manta_pay::types::{
    Checkpoint, FullIncomingNote, NullifierCommitment, OutgoingNote, PullResponse, ReceiverChunk,
    SenderChunk, Utxo, UtxoTransparency,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RpcMethods {
    pub version: u32,
    pub methods: Vec<String>,
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
    pub utxo_index: i64, // sqlite doesn't support u64
    pub utxo: Vec<u8>,
    pub full_incoming_note: Vec<u8>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Nullifier {
    pub idx: i64,
    pub nullifier_commitment: Vec<u8>,
    pub outgoing_note: Vec<u8>,
}

/// `DensePullResponse` is the dense design of raw `PullResponse`.
/// The reason for creating it is that raw `PullResponse` always carries a bunch of sender
/// and receiver chunks, which is quite not friendly for json serde and de-serde.
/// So with raw format, serialization will generates a large bottleneck.
/// So we will use `DensePullResponse` as transmission protocol.
///
/// Note:
/// Most of the time(90%+) is spent writing into json strings,
/// so the key design here is to improve the compression ratio of the chunks string ASAP.
/// If you want to improve, you can try more effective compression algorithm like `zstd`.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DensePullResponse {
    /// Same with raw `PullResponse`
    pub sender_receivers_total: u128,
    /// Total amount of dense `ReceiverChunk`
    pub receiver_len: usize,
    /// Compression of dense `ReceiverChunk`
    pub receivers: String,
    /// Total amount of dense `SenderChunk`
    pub sender_len: usize,
    /// Compression of dense `SenderChunk`
    pub senders: String,
    /// Same with raw `PullResponse`
    pub should_continue: bool,
    /// Next request checkpoint calculated from server.
    /// If should_continue = false, this data makes no sense.
    /// Else, the client can just use this one as next request cursor,
    /// It avoids complex computing on the client side,
    /// and the potential risk of inconsistent computing rules between the client and server
    pub next_checkpoint: Checkpoint,
}
