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

use anyhow::Result;
use codec::Encode;
use frame_support::{storage::storage_prefix, StorageHasher, Twox64Concat};
use manta_indexer::types::*;
use manta_xt::{dolphin_runtime, utils, MantaConfig};
use pallet_manta_pay::types::{
    Asset, AssetId, FullIncomingNote, IncomingNote, LightIncomingNote, UtxoCommitment,
};
use rand::{
    distributions::{Distribution, Uniform},
    thread_rng, Rng,
};
use sp_core::storage::StorageKey;

const SHARD_INDEX: u8 = 255;
const PALLET_NAME: &[u8] = b"MantaPay";
const SHARD_NAME: &[u8] = b"Shards";
const NULLIFIER_COMMITMENT_NAME: &[u8] = b"NullifierSetInsertionOrder";

pub fn create_map_key(storage_name: &[u8], key: impl Encode) -> StorageKey {
    let prefix = storage_prefix(PALLET_NAME, storage_name);
    let key = Twox64Concat::hash(&key.encode());

    let full_key = prefix.into_iter().chain(key).collect();
    StorageKey(full_key)
}

fn create_double_map_key(storage_name: &[u8], key1: impl Encode, key2: impl Encode) -> StorageKey {
    let prefix = storage_prefix(PALLET_NAME, storage_name);
    let key1 = Twox64Concat::hash(&key1.encode());
    let key2 = Twox64Concat::hash(&key2.encode());
    let key = key1.into_iter().chain(key2.into_iter());

    let full_key = prefix.into_iter().chain(key).collect();
    StorageKey(full_key)
}

fn generate_random_utxo() -> (Utxo, FullIncomingNote) {
    let mut full_note = FullIncomingNote::default();
    let uniform = Uniform::from(0..=SHARD_INDEX);
    let mut rng = thread_rng();
    let address_partition = uniform.sample(&mut rng);
    full_note.address_partition = address_partition;

    let mut incoming_note = IncomingNote::default();
    rng.fill(&mut incoming_note.ephemeral_public_key[..]);
    rng.fill(&mut incoming_note.tag[..]);
    rng.fill(&mut incoming_note.ciphertext[0][..]);
    rng.fill(&mut incoming_note.ciphertext[1][..]);
    rng.fill(&mut incoming_note.ciphertext[2][..]);

    let mut light_incoming_note = LightIncomingNote::default();
    rng.fill(&mut light_incoming_note.ephemeral_public_key[..]);
    rng.fill(&mut light_incoming_note.ciphertext[0][..]);
    rng.fill(&mut light_incoming_note.ciphertext[1][..]);
    rng.fill(&mut light_incoming_note.ciphertext[2][..]);

    full_note.incoming_note = incoming_note;
    full_note.light_incoming_note = light_incoming_note;

    // utxo
    let mut utxo = Utxo::default();
    let transparency = if address_partition % 2 == 0 {
        UtxoTransparency::Transparent
    } else {
        UtxoTransparency::Opaque
    };
    utxo.transparency = transparency;

    let asset_id = uniform.sample(&mut rng);
    let mut id = AssetId::default();
    id[31] = asset_id;
    let asset_value = uniform.sample(&mut rng) as u128 * 1000_000_000u128;
    let asset = Asset::new(id, asset_value);
    utxo.public_asset = asset;

    let mut commitment = UtxoCommitment::default();
    rng.fill(&mut commitment[..]);
    utxo.commitment = commitment;

    (utxo, full_note)
}

fn generate_random_nullifier_commitment() -> (NullifierCommitment, OutgoingNote) {
    let mut nullifier_commitment = NullifierCommitment::default();
    let mut rng = thread_rng();

    rng.fill(&mut nullifier_commitment[..]);

    let mut note = OutgoingNote::default();
    rng.fill(&mut note.ephemeral_public_key[..]);
    rng.fill(&mut note.ciphertext[0][..]);
    rng.fill(&mut note.ciphertext[1][..]);

    (nullifier_commitment, note)
}

async fn insert_one_million_utxos(ws: &str) -> Result<()> {
    let api = utils::create_manta_client::<MantaConfig>(&ws).await?;
    let seed = "//Alice";
    let signer = utils::create_signer_from_string::<MantaConfig, manta_xt::sr25519::Pair>(seed)?;

    let one_million = 1000_000u64;

    let utxo_count = one_million / 256; // each shard has this count of utxos
    let batch_size = 2100u64;
    let mut nullifier_index = 0u64;
    for shard_index in 0..=255u8 {
        let mut batch = vec![];
        for j in 0..utxo_count {
            let shard_prefix = create_double_map_key(SHARD_NAME, shard_index, j);
            let utxo = generate_random_utxo();
            let mut items = vec![(shard_prefix.as_ref().to_vec(), utxo.encode())];

            let nullifier_prefix = create_map_key(NULLIFIER_COMMITMENT_NAME, nullifier_index);
            let nullifier_commitment = generate_random_nullifier_commitment();
            items.push((
                nullifier_prefix.as_ref().to_vec(),
                nullifier_commitment.encode(),
            ));
            nullifier_index += 1;

            let storage_call =
                dolphin_runtime::runtime_types::frame_system::pallet::Call::set_storage { items };
            let storage_call =
                dolphin_runtime::runtime_types::dolphin_runtime::Call::System(storage_call);

            let sudo_call = dolphin_runtime::runtime_types::pallet_sudo::pallet::Call::sudo {
                call: Box::new(storage_call),
            };
            let sudo_call = dolphin_runtime::runtime_types::dolphin_runtime::Call::Sudo(sudo_call);

            batch.push(sudo_call);
        }
        let sudo_calls = dolphin_runtime::tx().utility().batch(batch);
        let block_hash = api
            .tx()
            .sign_and_submit_then_watch_default(&sudo_calls, &signer)
            .await
            .unwrap()
            .wait_for_in_block()
            .await
            .unwrap()
            .block_hash();
        println!("block hash: {block_hash}");
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let ws = "ws://127.0.0.1:9800";
    insert_one_million_utxos(ws).await
}
