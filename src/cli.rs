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

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct IndexerCli {
    /// The path points to the file for setup logging and configuration for indexer.
    /// This path should contains files with a hardcoded name like config.toml and log.yaml
    #[arg(short, long, default_value_t=String::from("./conf"))]
    pub config_path: String,

    /// The path points to the sql migrations with a hardcoded name like 0_schema.sql
    #[arg(short, long, default_value_t=String::from("./migrations"))]
    pub migrations_path: String,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

// todo
#[derive(Subcommand, Debug)]
pub enum Commands {
    PullUtxo,
    InsertUtxo,
}
