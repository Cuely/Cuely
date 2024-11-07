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
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

pub mod dummy;
pub use dummy::DummyQuery;

pub mod host_links;
pub mod id2node;
pub mod links;
pub mod phrase_or_term;

pub use host_links::HostLinksQuery;
pub use id2node::Id2NodeQuery;
pub use links::LinksQuery;
pub use phrase_or_term::PhraseOrTermQuery;