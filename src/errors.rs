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

use thiserror::Error;

#[derive(Debug, Error)]
pub enum IndexerError {
    #[error("The resuest is timeout.")]
    RequestTimeOut,
    #[error("Seems full node doesn't work.")]
    FullNodeIsDown,
    #[error("Failed to fetch data from db due to: {0}.")]
    DbFetchError(#[from] sqlx::Error),
    #[error("Wrong merkle tree index.")]
    WrongMerkleTreeIndex,
    #[error("Failed to decode as codec foramt.")]
    DecodedError,
    #[error("JsonRpsee Error: {0}.")]
    JsonRpseeError(#[from] jsonrpsee::core::error::Error),
    #[error("Wrong config file(config.toml).")]
    WrongConfig,
    #[error("This rpc method doesn't exist.")]
    RpcMethodNotExists,
    #[error("Unknown error.")]
    Unknown,
}
