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

use std::sync::Arc;

use optics::PatternPart;
use tantivy::{
    fieldnorm::FieldNormReader,
    postings::SegmentPostings,
    query::{EmptyScorer, Explanation, Scorer},
    schema::IndexRecordOption,
    tokenizer::Tokenizer,
    DocId, DocSet, Postings, Score, SegmentReader, TantivyError, TERMINATED,
};

use crate::{
    fastfield_cache::FastFieldCache,
    query::intersection::Intersection,
    ranking::bm25::Bm25Weight,
    schema::{FastField, Field, TextField, ALL_FIELDS},
};

#[derive(Clone)]
pub struct PatternQuery {
    patterns: Vec<PatternPart>,
    field: tantivy::schema::Field,
    raw_terms: Vec<tantivy::Term>,
    fastfield_cache: Arc<FastFieldCache>,
}

impl std::fmt::Debug for PatternQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PatternQuery")
            .field("patterns", &self.patterns)
            .field("field", &self.field)
            .field("raw_terms", &self.raw_terms)
            .finish()
    }
}

impl PatternQuery {
    pub fn new(
        patterns: Vec<PatternPart>,
        field: TextField,
        schema: &tantivy::schema::Schema,
        fastfield_cache: Arc<FastFieldCache>,
    ) -> Self {
        let mut raw_terms = Vec::new();

        let tv_field = schema.get_field(Field::Text(field).name()).unwrap();

        for pattern in &patterns {
            match pattern {
                PatternPart::Raw(text) => {
                    let mut stream = field.tokenizer().token_stream(text);

                    while let Some(token) = stream.next() {
                        let term = tantivy::Term::from_field_text(tv_field, &token.text);
                        raw_terms.push(term);
                    }
                }
                PatternPart::Wildcard => {}
                PatternPart::Anchor => {}
            }
        }

        Self {
            patterns,
            field: tv_field,
            raw_terms,
            fastfield_cache,
        }
    }
}

impl tantivy::query::Query for PatternQuery {
    fn weight(
        &self,
        scoring: tantivy::query::EnableScoring<'_>,
    ) -> tantivy::Result<Box<dyn tantivy::query::Weight>> {
        let bm25_weight = match scoring {
            tantivy::query::EnableScoring::Enabled(searcher) => {
                Some(Bm25Weight::for_terms(searcher, &self.raw_terms)?)
            }
            tantivy::query::EnableScoring::Disabled(_) => None,
        };

        Ok(Box::new(PatternWeight {
            similarity_weight: bm25_weight,
            raw_terms: self.raw_terms.clone(),
            patterns: self.patterns.clone(),
            field: self.field,
            fastfield_cache: self.fastfield_cache.clone(),
        }))
    }

    fn query_terms<'a>(&'a self, visitor: &mut dyn FnMut(&'a tantivy::Term, bool)) {
        for term in &self.raw_terms {
            visitor(term, true);
        }
    }
}

enum SmallPatternPart {
    Term,
    Wildcard,
    Anchor,
}

struct PatternWeight {
    similarity_weight: Option<Bm25Weight>,
    patterns: Vec<PatternPart>,
    raw_terms: Vec<tantivy::Term>,
    field: tantivy::schema::Field,
    fastfield_cache: Arc<FastFieldCache>,
}

impl PatternWeight {
    fn fieldnorm_reader(&self, reader: &SegmentReader) -> tantivy::Result<FieldNormReader> {
        if self.similarity_weight.is_some() {
            if let Some(fieldnorm_reader) = reader.fieldnorms_readers().get_field(self.field)? {
                return Ok(fieldnorm_reader);
            }
        }
        Ok(FieldNormReader::constant(reader.max_doc(), 1))
    }

    pub(crate) fn pattern_scorer(
        &self,
        reader: &SegmentReader,
        boost: Score,
    ) -> tantivy::Result<Option<PatternScorer>> {
        let similarity_weight = if let Some(weight) = &self.similarity_weight {
            weight.boost_by(boost)
        } else {
            return Ok(None);
        };

        let fieldnorm_reader = self.fieldnorm_reader(reader)?;
        let mut term_postings_list = Vec::new();

        for term in &self.raw_terms {
            if let Some(postings) = reader
                .inverted_index(term.field())?
                .read_postings(term, IndexRecordOption::WithFreqsAndPositions)?
            {
                term_postings_list.push(postings);
            } else {
                return Ok(None);
            }
        }

        let small_patterns = self
            .patterns
            .iter()
            .map(|pattern| match pattern {
                PatternPart::Raw(_) => SmallPatternPart::Term,
                PatternPart::Wildcard => SmallPatternPart::Wildcard,
                PatternPart::Anchor => SmallPatternPart::Anchor,
            })
            .collect();

        let num_tokens_fastfield = match &ALL_FIELDS[self.field.field_id() as usize] {
            Field::Text(TextField::Title) => Ok(FastField::NumTitleTokens),
            Field::Text(TextField::CleanBody) => Ok(FastField::NumCleanBodyTokens),
            Field::Text(TextField::Url) => Ok(FastField::NumUrlTokens),
            Field::Text(TextField::Domain) => Ok(FastField::NumDomainTokens),
            Field::Text(TextField::Site) => Ok(FastField::NumSiteTokens),
            Field::Text(TextField::Description) => Ok(FastField::NumDescriptionTokens),
            Field::Text(TextField::FlattenedSchemaOrgJson) => {
                Ok(FastField::NumFlattenedSchemaTokens)
            }
            field => Err(TantivyError::InvalidArgument(format!(
                "{} is not supported in pattern query",
                field.name()
            ))),
        }?;

        Ok(Some(PatternScorer::new(
            similarity_weight,
            term_postings_list,
            fieldnorm_reader,
            small_patterns,
            reader.segment_id(),
            num_tokens_fastfield,
            self.fastfield_cache.clone(),
        )))
    }
}

impl tantivy::query::Weight for PatternWeight {
    fn scorer(
        &self,
        reader: &tantivy::SegmentReader,
        boost: tantivy::Score,
    ) -> tantivy::Result<Box<dyn tantivy::query::Scorer>> {
        if let Some(scorer) = self.pattern_scorer(reader, boost)? {
            Ok(Box::new(scorer))
        } else {
            Ok(Box::new(EmptyScorer))
        }
    }

    fn explain(
        &self,
        reader: &tantivy::SegmentReader,
        doc: tantivy::DocId,
    ) -> tantivy::Result<tantivy::query::Explanation> {
        let scorer_opt = self.pattern_scorer(reader, 1.0)?;
        if scorer_opt.is_none() {
            return Err(TantivyError::InvalidArgument(format!(
                "Document #({doc}) does not match"
            )));
        }
        let mut scorer = scorer_opt.unwrap();
        if scorer.seek(doc) != doc {
            return Err(TantivyError::InvalidArgument(format!(
                "Document #({doc}) does not match"
            )));
        }
        let fieldnorm_reader = self.fieldnorm_reader(reader)?;
        let fieldnorm_id = fieldnorm_reader.fieldnorm_id(doc);
        let phrase_count = scorer.phrase_count();
        let mut explanation = Explanation::new("Pattern Scorer", scorer.score());
        explanation.add_detail(
            self.similarity_weight
                .as_ref()
                .unwrap()
                .explain(fieldnorm_id, phrase_count),
        );
        Ok(explanation)
    }
}

struct PatternScorer {
    similarity_weight: Bm25Weight,
    fieldnorm_reader: FieldNormReader,
    intersection_docset: Intersection<SegmentPostings>,
    pattern: Vec<SmallPatternPart>,
    num_query_terms: usize,
    left: Vec<u32>,
    right: Vec<u32>,
    phrase_count: u32,
    segment: tantivy::SegmentId,
    num_tokens_field: FastField,
    fastfield_cache: Arc<FastFieldCache>,
}

impl PatternScorer {
    fn new(
        similarity_weight: Bm25Weight,
        term_postings_list: Vec<SegmentPostings>,
        fieldnorm_reader: FieldNormReader,
        pattern: Vec<SmallPatternPart>,
        segment: tantivy::SegmentId,
        num_tokens_field: FastField,
        fastfield_cache: Arc<FastFieldCache>,
    ) -> Self {
        let num_query_terms = term_postings_list.len();

        Self {
            intersection_docset: Intersection::new(term_postings_list),
            num_query_terms,
            similarity_weight,
            fieldnorm_reader,
            pattern,
            left: Vec::with_capacity(100),
            right: Vec::with_capacity(100),
            phrase_count: 0,
            segment,
            num_tokens_field,
            fastfield_cache,
        }
    }
    fn phrase_count(&self) -> u32 {
        self.phrase_count
    }

    fn pattern_match(&mut self) -> bool {
        self.phrase_count = self.perform_pattern_match() as u32;

        self.phrase_count > 0
    }

    fn perform_pattern_match(&mut self) -> usize {
        {
            self.intersection_docset
                .docset_mut_specialized(0)
                .positions(&mut self.left);
        }

        let mut intersection_len = self.left.len();
        let mut out = Vec::new();

        let mut current_right_term = 1;
        let mut slop = 1;
        let num_tokens_doc = self
            .fastfield_cache
            .get_segment(&self.segment)
            .get_doc_cache(&self.num_tokens_field)
            .get_u64(&self.doc())
            .unwrap();

        for (i, pattern_part) in self.pattern.iter().enumerate().skip(1) {
            match pattern_part {
                SmallPatternPart::Term => {
                    {
                        self.intersection_docset
                            .docset_mut_specialized(current_right_term)
                            .positions(&mut self.right);
                    }
                    out.resize(self.left.len().max(self.right.len()), 0);
                    intersection_len =
                        intersection_with_slop(&self.left[..], &self.right[..], &mut out, slop);

                    slop = 1;

                    if intersection_len == 0 {
                        return 0;
                    }

                    self.left = out[..intersection_len].to_vec();
                    out = Vec::new();
                    current_right_term += 1;
                }
                SmallPatternPart::Wildcard => {
                    slop = u32::MAX;
                }
                SmallPatternPart::Anchor if i == 0 => {
                    if let Some(pos) = self.left.first() {
                        if *pos != 0 {
                            return 0;
                        }
                    }
                }
                SmallPatternPart::Anchor if i == self.pattern.len() - 1 => {
                    {
                        self.intersection_docset
                            .docset_mut_specialized(self.num_query_terms - 1)
                            .positions(&mut self.right);
                    }

                    if let Some(pos) = self.right.last() {
                        if *pos != (num_tokens_doc - 1) as u32 {
                            return 0;
                        }
                    }
                }
                SmallPatternPart::Anchor => {}
            }
        }

        intersection_len
    }
}

impl Scorer for PatternScorer {
    fn score(&mut self) -> Score {
        let doc = self.doc();
        let fieldnorm_id = self.fieldnorm_reader.fieldnorm_id(doc);
        self.similarity_weight
            .score(fieldnorm_id, self.phrase_count())
    }
}

impl DocSet for PatternScorer {
    fn advance(&mut self) -> DocId {
        loop {
            let doc = self.intersection_docset.advance();
            if doc == TERMINATED || self.pattern_match() {
                return doc;
            }
        }
    }

    fn seek(&mut self, target: DocId) -> DocId {
        debug_assert!(target >= self.doc());
        let doc = self.intersection_docset.seek(target);
        if doc == TERMINATED || self.pattern_match() {
            return doc;
        }
        self.advance()
    }

    fn doc(&self) -> tantivy::DocId {
        self.intersection_docset.doc()
    }

    fn size_hint(&self) -> u32 {
        self.intersection_docset.size_hint()
    }
}

/// Intersect twos sorted arrays `left` and `right` and outputs the
/// resulting array in `out`. The positions in out are all positions from right where
/// the distance to left_pos <= slop
///
/// Returns the length of the intersection
fn intersection_with_slop(left: &[u32], right: &[u32], out: &mut [u32], slop: u32) -> usize {
    let mut left_index = 0;
    let mut right_index = 0;
    let mut count = 0;
    let left_len = left.len();
    let right_len = right.len();
    while left_index < left_len && right_index < right_len {
        let left_val = left[left_index];
        let right_val = right[right_index];

        // The three conditions are:
        // left_val < right_slop -> left index increment.
        // right_slop <= left_val <= right -> find the best match.
        // left_val > right -> right index increment.
        let right_slop = if right_val >= slop {
            right_val - slop
        } else {
            0
        };

        if left_val < right_slop {
            left_index += 1;
        } else if right_slop <= left_val && left_val <= right_val {
            while left_index + 1 < left_len {
                // there could be a better match
                let next_left_val = left[left_index + 1];
                if next_left_val > right_val {
                    // the next value is outside the range, so current one is the best.
                    break;
                }
                // the next value is better.
                left_index += 1;
            }
            // store the match in left.
            out[count] = right_val;
            count += 1;
            right_index += 1;
        } else if left_val > right_val {
            right_index += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aux_intersection(left: &[u32], right: &[u32], expected: &[u32], slop: u32) {
        let mut out = vec![0; left.len().max(right.len())];

        let intersection_size = intersection_with_slop(left, right, &mut out, slop);

        assert_eq!(&out[..intersection_size], expected);
    }

    #[test]
    fn test_intersection_with_slop() {
        aux_intersection(&[20, 75, 77], &[18, 21, 60], &[21, 60], u32::MAX);
        aux_intersection(&[21, 60], &[50, 61], &[61], 1);

        aux_intersection(&[1, 2, 3], &[], &[], 1);
        aux_intersection(&[], &[1, 2, 3], &[], 1);

        aux_intersection(&[1, 2, 3], &[4, 5, 6], &[4], 1);
        aux_intersection(&[1, 2, 3], &[4, 5, 6], &[4, 5, 6], u32::MAX);

        aux_intersection(&[20, 75, 77], &[18, 21, 60], &[21, 60], u32::MAX);
        aux_intersection(&[21, 60], &[61, 62], &[61, 62], 2);

        aux_intersection(&[60], &[61, 62], &[61, 62], 2);
    }
}