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

pub const CIPHER_TEXT_LENGTH: usize = 68;
pub const EPHEMERAL_PUBLIC_KEY_LENGTH: usize = 32;
pub const UTXO_LENGTH: usize = 32;
pub const VOID_NUMBER_LENGTH: usize = 32;

/// Void Number Type
pub type VoidNumber = [u8; VOID_NUMBER_LENGTH];

/// UTXO Type
pub type Utxo = [u8; UTXO_LENGTH];

/// Group Type
pub type Group = [u8; EPHEMERAL_PUBLIC_KEY_LENGTH];

/// Ciphertext Type
pub type Ciphertext = [u8; CIPHER_TEXT_LENGTH];

/// Receiver Chunk Data Type
pub type ReceiverChunk = Vec<(Utxo, EncryptedNote)>; // The size of each single element should be 132 bytes

/// Sender Chunk Data Type
pub type SenderChunk = Vec<VoidNumber>;

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

#[derive(Clone, Debug, Decode, Encode, Deserialize, Serialize, sqlx::Decode, sqlx::Encode)]
pub struct EncryptedNote {
    /// Ephemeral Public Key
    pub ephemeral_public_key: Group,

    /// Ciphertext
    #[serde(
        with = "manta_util::serde_with::As::<[manta_util::serde_with::Same; CIPHER_TEXT_LENGTH]>"
    )]
    pub ciphertext: Ciphertext,
}

#[derive(Clone, Debug, Decode, Encode, Deserialize, Serialize)]
pub struct PullResponse {
    /// Pull Continuation Flag
    ///
    /// The `should_continue` flag is set to `true` if the client should request more data from the
    /// ledger to finish the pull.
    pub should_continue: bool,

    /// Ledger Receiver Chunk
    pub receivers: ReceiverChunk,

    /// Ledger Sender Chunk
    pub senders: SenderChunk,

    /// Total Number of (Senders + Receivers) in Ledger
    pub senders_receivers_total: u128,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Shard {
    pub shard_index: u8,
    pub next_index: i64,
    // utxo: (Utxo, EncryptedNote),
    pub utxo: Vec<u8>,
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
}

impl From<PullResponse> for DensePullResponse {
    fn from(raw: PullResponse) -> Self {
        let receiver_bytes_len =
            raw.receivers.len() * (mem::size_of::<Utxo>() + mem::size_of::<EncryptedNote>());
        let sender_bytes_len = raw.senders.len() * mem::size_of::<VoidNumber>();
        let mut receivers_bytes = Vec::with_capacity(receiver_bytes_len);
        let mut sender_bytes = Vec::with_capacity(sender_bytes_len);

        raw.receivers.iter().for_each(|item| {
            // TODO use more high level api like parity::codec::Encode, but need to make sure the compress behavior is same.
            receivers_bytes.extend_from_slice(item.0.as_slice());
            receivers_bytes.extend_from_slice(item.1.ephemeral_public_key.as_slice());
            receivers_bytes.extend_from_slice(item.1.ciphertext.as_slice());
        });
        raw.senders.iter().for_each(|item| {
            sender_bytes.extend_from_slice(item.as_slice());
        });

        Self {
            sender_receivers_total: raw.senders_receivers_total,
            receiver_len: raw.receivers.len(),
            receivers: base64::encode(receivers_bytes),
            sender_len: raw.senders.len(),
            senders: base64::encode(sender_bytes),
            should_continue: raw.should_continue,
        }
    }
}
