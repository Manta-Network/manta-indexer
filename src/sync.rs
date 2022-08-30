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

use crate::types::PullResponse;
use anyhow::Result;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};

pub async fn synchronize_shards(ws: &WsClient) -> Result<PullResponse> {
    todo!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_shards_should_work() {
        assert!(true);
    }

    #[tokio::test]
    async fn wss_dummy_test() {
        crate::utils::init_logger();

        let url = "wss://falafel.calamari.systems:443";
        let client = crate::utils::create_ws_client(url).await.unwrap();
        let response: String = client.request("system_chain", None).await.unwrap();
        assert_eq!(&response, "Calamari Parachain");
    }
}
