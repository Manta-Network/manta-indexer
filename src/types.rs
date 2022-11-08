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

#[derive(Clone, Debug, Decode, Encode, Deserialize, Serialize, sqlx::Decode, sqlx::Encode)]
pub struct EncryptedNote {
    /// Ephemeral Public Key
    pub ephemeral_public_key: Group,

    /// Ciphertext
    #[serde(with = "self::arrays")]
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

// Currently serde::Deserialize doesn't support an array whose length is bigger than 16.
// API: https://docs.rs/serde/latest/serde/trait.Deserialize.html#
// Track issue: https://github.com/serde-rs/serde/issues/1937
pub mod arrays {
    use std::{convert::TryInto, marker::PhantomData};

    use serde::{
        de::{SeqAccess, Visitor},
        ser::SerializeTuple,
        Deserialize, Deserializer, Serialize, Serializer,
    };
    pub fn serialize<S: Serializer, T: Serialize, const N: usize>(
        data: &[T; N],
        ser: S,
    ) -> Result<S::Ok, S::Error> {
        let mut s = ser.serialize_tuple(N)?;
        for item in data {
            s.serialize_element(item)?;
        }
        s.end()
    }

    struct ArrayVisitor<T, const N: usize>(PhantomData<T>);

    impl<'de, T, const N: usize> Visitor<'de> for ArrayVisitor<T, N>
    where
        T: Deserialize<'de>,
    {
        type Value = [T; N];

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str(&format!("an array of length {}", N))
        }

        #[inline]
        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            // can be optimized using MaybeUninit
            let mut data = Vec::with_capacity(N);
            for _ in 0..N {
                match (seq.next_element())? {
                    Some(val) => data.push(val),
                    None => return Err(serde::de::Error::invalid_length(N, &self)),
                }
            }
            match data.try_into() {
                Ok(arr) => Ok(arr),
                Err(_) => unreachable!(),
            }
        }
    }
    pub fn deserialize<'de, D, T, const N: usize>(deserializer: D) -> Result<[T; N], D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        deserializer.deserialize_tuple(N, ArrayVisitor::<T, N>(PhantomData))
    }
}
