// Stract is an open source web search engine.
// Copyright (C) 2024 Stract ApS
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
// along with this program.  If not, see <https://www.gnu.org/licenses/>

pub mod api;
mod raft;

use api::{Get, Set};
use std::sync::Arc;

use openraft::{BasicNode, Raft, RaftNetworkFactory};

use crate::sonic_service;

use self::raft::RemoteClient;

use super::{store::StateMachineStore, NodeId, TypeConfig};

#[derive(Clone)]
pub struct Network;

impl RaftNetworkFactory<TypeConfig> for Network {
    type Network = RemoteClient;

    async fn new_client(&mut self, target: NodeId, node: &BasicNode) -> Self::Network {
        RemoteClient::new(target, node.clone())
    }
}

pub type AppendEntriesRequest = openraft::raft::AppendEntriesRequest<TypeConfig>;
pub type AppendEntriesResponse = openraft::raft::AppendEntriesResponse<NodeId>;

pub type InstallSnapshotRequest = openraft::raft::InstallSnapshotRequest<TypeConfig>;
pub type InstallSnapshotResponse = openraft::raft::InstallSnapshotResponse<NodeId>;

pub type VoteRequest = openraft::raft::VoteRequest<NodeId>;
pub type VoteResponse = openraft::raft::VoteResponse<NodeId>;

sonic_service!(
    Server,
    [
        AppendEntriesRequest,
        InstallSnapshotRequest,
        VoteRequest,
        Get,
        Set
    ]
);

pub struct Server {
    raft: Raft<TypeConfig>,
    state_machine_store: Arc<StateMachineStore>,
}

impl Server {
    pub fn new(raft: Raft<TypeConfig>, state_machine_store: Arc<StateMachineStore>) -> Self {
        Self {
            raft,
            state_machine_store,
        }
    }
}
