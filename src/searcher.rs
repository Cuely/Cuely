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

use std::str::FromStr;

use uuid::Uuid;

use crate::entity_index::{EntityIndex, StoredEntity};
use crate::image_store::Image;
use crate::index::Index;
use crate::inverted_index::InvertedIndexSearchResult;
use crate::query::Query;
use crate::ranking::Ranker;
use crate::webpage::Url;
use crate::Result;

#[derive(Debug)]
pub struct SearchResult {
    pub spell_corrected_query: Option<String>,
    pub webpages: InvertedIndexSearchResult,
    pub entity: Option<StoredEntity>,
}

pub struct Searcher {
    index: Index,
    entity_index: Option<EntityIndex>,
}

impl From<Index> for Searcher {
    fn from(index: Index) -> Self {
        Self::new(index, None)
    }
}

impl Searcher {
    pub fn new(index: Index, entity_index: Option<EntityIndex>) -> Self {
        Searcher {
            index,
            entity_index,
        }
    }
}

impl Searcher {
    pub fn search(&self, query: &str) -> Result<SearchResult> {
        let raw_query = query.to_string();
        let query = Query::parse(query, self.index.schema(), self.index.tokenizers())?;
        let ranker = Ranker::new(query.clone());
        let webpages = self.index.search(&query, ranker.collector())?;
        let correction = self.index.spell_correction(&query.simple_terms());

        let entity = self
            .entity_index
            .as_ref()
            .and_then(|index| index.search(raw_query));

        Ok(SearchResult {
            webpages,
            entity,
            spell_corrected_query: correction,
        })
    }

    pub fn favicon(&self, site: &Url) -> Option<Image> {
        self.index.retrieve_favicon(site)
    }

    pub fn primary_image(&self, uuid: String) -> Option<Image> {
        if let Ok(uuid) = Uuid::from_str(uuid.as_str()) {
            return self.index.retrieve_primary_image(&uuid);
        }
        None
    }

    pub fn entity_image(&self, entity: String) -> Option<Image> {
        self.entity_index
            .as_ref()
            .and_then(|index| index.retrieve_image(&entity))
    }

    pub fn attribute_occurrence(&self, attribute: &String) -> Option<u32> {
        self.entity_index
            .as_ref()
            .and_then(|index| index.get_attribute_occurrence(attribute))
    }
}
