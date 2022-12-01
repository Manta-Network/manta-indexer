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
    pub utxo_index: i64, // sqlite doesn't support u64
    pub utxo: Vec<u8>,
    pub full_incoming_note: Vec<u8>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Nullifier {
    pub nullifier_commitment: Vec<u8>,
    pub outgoing_note: Vec<u8>,
}
