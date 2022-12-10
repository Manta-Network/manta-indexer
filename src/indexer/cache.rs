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

#![allow(dead_code)]

use lru::LruCache;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use tokio::sync::Mutex;

/// Lru key used in sender and receiver format:
/// [4 bit] type prefix + [12 bit] sub field value + [48 bit] field value.
type LruKeyType = u64;

const TYPE_PREFIX_BITS: u32 = 4;
const SUB_FIELD_BITS: u32 = 12;
const VALUE_FIELD_BITS: u32 = 48;

const RECEIVER_KEY_PREFIX: u64 = 1;
const SENDER_KEY_PREFIX: u64 = 2;

const TYPE_PREFIX_MASK: u64 = !(LruKeyType::MAX >> TYPE_PREFIX_BITS);
const SUB_FIELD_MASK: u64 =
    (LruKeyType::MAX >> TYPE_PREFIX_BITS) & (LruKeyType::MAX << VALUE_FIELD_BITS);
const VALUE_MASK: u64 = !(LruKeyType::MAX << VALUE_FIELD_BITS);

/// For generating a receiver lru key from its shard_index and utxo_index with rule of `LruKeyType`
#[inline]
pub(crate) fn receiver_key(shard_idx: LruKeyType, utxo_idx: LruKeyType) -> LruKeyType {
    (RECEIVER_KEY_PREFIX.wrapping_shl(SUB_FIELD_BITS + VALUE_FIELD_BITS) & TYPE_PREFIX_MASK)
        | (shard_idx.wrapping_shl(VALUE_FIELD_BITS) & SUB_FIELD_MASK)
        | (utxo_idx & VALUE_MASK)
}

/// For generating a sender lru key from its index with rule of `LruKeyType`.
#[inline]
pub(crate) fn sender_key(idx: LruKeyType) -> LruKeyType {
    (SENDER_KEY_PREFIX.wrapping_shl(SUB_FIELD_BITS + VALUE_FIELD_BITS) & TYPE_PREFIX_MASK)
        | (idx & VALUE_MASK)
}

struct ShardThreadSafeLru {
    shard_num: usize,
    maps: Vec<Mutex<LruCache<LruKeyType, Vec<u8>>>>,
}

impl ShardThreadSafeLru {
    async fn batch_get(&self, keys: Vec<LruKeyType>) -> Vec<Option<Vec<u8>>> {
        let mut result = std::iter::repeat(None)
            .take(keys.len())
            .collect::<Vec<Option<Vec<u8>>>>();
        let mut shard_keys = HashMap::new();
        for (idx, key) in keys.into_iter().enumerate() {
            let shard_id = key as usize % self.shard_num;
            shard_keys
                .entry(shard_id)
                .or_insert(vec![])
                .push((idx, key));
        }
        for (shard, keys) in shard_keys {
            let mut cache = self.maps[shard].lock().await;
            for key in keys {
                result[key.0] = cache.get(&key.1).cloned();
            }
        }
        result
    }

    async fn batch_put(&self, v: Vec<(LruKeyType, Vec<u8>)>) {
        let mut shard_vs = HashMap::new();
        for (k, v) in v {
            let shard_id = k as usize % self.shard_num;
            shard_vs.entry(shard_id).or_insert(vec![]).push((k, v));
        }
        for (shard, vs) in shard_vs {
            let mut cache = self.maps[shard].lock().await;
            for (k, v) in vs {
                cache.put(k, v);
            }
        }
    }
}

/// Used for cache receiver and sender ledger data.
/// Storage data is parity codec format Vec<u8> for simplify.
pub(crate) static LEDGER_LRU: Lazy<Mutex<LruCache<LruKeyType, Vec<u8>>>> =
    Lazy::new(|| Mutex::new(LruCache::new(NonZeroUsize::new(1024 * 1024 * 16).unwrap())));

/// A sharded lru cache for better concurrency.
/// When the data size grows bigger, consider use this(not for now).
/// If data size is not so big, the single shard cache performance is better.
static LEDGER_LRU2: Lazy<ShardThreadSafeLru> = Lazy::new(|| {
    let shard_num = 32;
    let mut maps = Vec::with_capacity(32);
    for _ in 0..shard_num {
        maps.push(Mutex::new(LruCache::new(
            NonZeroUsize::new(512 * 1024).unwrap(),
        )));
    }
    ShardThreadSafeLru { shard_num, maps }
});

pub(crate) async fn get_receiver(shard_idx: u8, utxo_idx: usize) -> Option<Vec<u8>> {
    let key = receiver_key(shard_idx as LruKeyType, utxo_idx as LruKeyType);
    get_batch_lru(vec![key]).await.pop().unwrap_or_default()
}

pub(crate) async fn get_sender(sender_index: usize) -> Option<Vec<u8>> {
    let key = sender_key(sender_index as LruKeyType);
    get_batch_lru(vec![key]).await.pop().unwrap_or_default()
}

pub(crate) async fn get_batch_shard_receiver(
    shard_idx: u8,
    utxo_idxs: &Vec<usize>,
) -> Vec<Option<Vec<u8>>> {
    let mut keys = Vec::with_capacity(utxo_idxs.len());
    for idx in utxo_idxs {
        keys.push(receiver_key(shard_idx as LruKeyType, *idx as LruKeyType));
    }
    get_batch_lru(keys).await
}

pub(crate) async fn get_batch_sender(sender_idxs: &Vec<usize>) -> Vec<Option<Vec<u8>>> {
    let mut keys = Vec::with_capacity(sender_idxs.len());
    for idx in sender_idxs.into_iter() {
        keys.push(sender_key(*idx as LruKeyType));
    }
    get_batch_lru(keys).await
}

async fn get_batch_lru(keys: Vec<LruKeyType>) -> Vec<Option<Vec<u8>>> {
    let mut result = Vec::with_capacity(keys.len());
    let mut cache = LEDGER_LRU.lock().await;
    keys.into_iter()
        .for_each(|key| result.push(cache.get(&key).cloned()));
    result
}

pub(crate) async fn put_receiver(shard_idx: u8, utxo_idx: usize, r: Vec<u8>) {
    let key = receiver_key(shard_idx as LruKeyType, utxo_idx as LruKeyType);
    put_batch_lru(vec![(key, r)]).await;
}

pub(crate) async fn put_sender(sender_idx: u8, s: Vec<u8>) {
    let key = sender_key(sender_idx as LruKeyType);
    put_batch_lru(vec![(key, s)]).await;
}

pub(crate) async fn put_batch_receiver(shard_idx: u8, r: Vec<(usize, Vec<u8>)>) {
    let mut batch = Vec::with_capacity(r.len());
    for idx in r.into_iter() {
        batch.push((
            receiver_key(shard_idx as LruKeyType, idx.0 as LruKeyType),
            idx.1,
        ));
    }
    put_batch_lru(batch).await;
}

pub(crate) async fn put_batch_sender(s: Vec<(usize, Vec<u8>)>) {
    let mut batch = Vec::with_capacity(s.len());
    for v in s.into_iter() {
        batch.push((sender_key(v.0 as LruKeyType), v.1));
    }
    put_batch_lru(batch).await;
}

async fn put_batch_lru(v: Vec<(LruKeyType, Vec<u8>)>) {
    let mut cache = LEDGER_LRU.lock().await;
    v.into_iter().for_each(|(k, v)| {
        cache.put(k, v);
    });
}

pub(crate) async fn lru_size() -> usize {
    let mut cache = LEDGER_LRU.lock().await;
    cache.len()
}

#[cfg(test)]
mod tests {
    use crate::indexer::cache::{receiver_key, sender_key};

    #[test]
    fn test_key() {
        assert_eq!(receiver_key(10, 234523), 1155736254374188059);
        assert_eq!(sender_key(234588), 2305843009213928540);
    }
}
