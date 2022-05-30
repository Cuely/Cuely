// Cuely is an open source web search engine.
// Copyright (C) 2022 Cuely ApS
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
use cuely::{Config, Indexer, Mode};
use std::fs;

#[tokio::main]
async fn main() {
    let raw_config =
        fs::read_to_string("configs/indexer.toml").expect("Failed to read config file");

    let config: Config = toml::from_str(&raw_config).expect("Failed to parse config");

    match config.mode {
        Mode::Indexer => {
            Indexer::from_config(config)
                .run()
                .await
                .expect("Failed to index documents");
        }
    }
}
