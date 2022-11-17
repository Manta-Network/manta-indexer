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

// use criterion::{black_box, criterion_group, criterion_main, Criterion};
// use rand::prelude::*;
// use sqlx::sqlite::SqlitePool;

// fn db_read_bench(pool: &SqlitePool) {
//     let mut stmt = conn.prepare("SELECT shard_index, next_index, utxo FROM shards WHERE shard_index >= (?1) and next_index = (?2)").unwrap();
//     let mut stmt = conn
//         .prepare("SELECT shard_index, next_index, utxo FROM shards WHERE shard_index >= (?1)")
//         .unwrap();
// }
//
// fn criterion_benchmark(c: &mut Criterion) {
//     let db_path = "shards.db";
//     let pool = SqlitePool::open(db_path).unwrap();
//
//     c.bench_function("shards reading", |b| b.iter(|| db_read_bench(&pool)));
// }
//
// criterion_group!(benches, criterion_benchmark);
// criterion_main!(benches);

fn main() {}
