// Copyright 2020-2023 Manta Network.
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

/// The storage name: MantaPay
pub const MANTA_PAY_KEY_PREFIX: [u8; 8] = *b"MantaPay";
/// The storage name: Shards
pub const MANTA_PAY_STORAGE_SHARDS_NAME: [u8; 6] = *b"Shards";
/// The storage name: NullifierSetInsertionOrder
pub const MANTA_PAY_STORAGE_NULLIFIER_NAME: [u8; 26] = *b"NullifierSetInsertionOrder";

pub const MEGABYTE: u32 = 1024 * 1024;

pub const PULL_LEDGER_DIFF_METHODS: &str = "mantaPay_pull_ledger_diff";
