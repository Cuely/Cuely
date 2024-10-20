#![doc(html_logo_url = "http://fulmicoton.com/tantivy-logo/tantivy-logo.png")]
#![doc(test(attr(allow(unused_variables), deny(warnings))))]
#![allow(
    clippy::len_without_is_empty,
    clippy::derive_partial_eq_without_eq,
    clippy::module_inception,
    clippy::needless_range_loop,
    clippy::bool_assert_comparison
)]

//! # `tantivy`
//!
//! Tantivy is a search engine library.
//! Think `Lucene`, but in Rust.
//!
//! A good place for you to get started is to check out
//! the example code (
//! [literate programming](https://tantivy-search.github.io/examples/basic_search.html) /
//! [source code](https://github.com/quickwit-oss/tantivy/blob/main/examples/basic_search.rs))
//!
//! # Tantivy Architecture Overview
//!
//! Tantivy is inspired by Lucene, the Architecture is very similar.
//!
//! ## Core Concepts
//!
//! - **[Index]**: A collection of segments. The top level entry point for tantivy users to search
//!   and index data.
//!
//! - **[Segment]**: At the heart of Tantivy's indexing structure is the [Segment]. It contains
//!   documents and indices and is the atomic unit of indexing and search.
//!
//! - **[Schema](schema)**: A schema is a set of fields in an index. Each field has a specific data
//!   type and set of attributes.
//!
//! - **[IndexWriter]**: Responsible creating and merging segments. It executes the indexing
//!   pipeline including tokenization, creating indices, and storing the index in the
//!   [Directory](directory).
//!
//! - **Searching**: [Searcher] searches the segments with anything that implements
//!   [Query](query::Query) and merges the results. The list of [supported
//!     queries](query::Query#implementors). Custom Queries are supported by implementing the
//!     [Query](query::Query) trait.
//!
//! - **[Directory](directory)**: Abstraction over the storage where the index data is stored.
//!
//! - **[Tokenizer](tokenizer)**: Breaks down text into individual tokens. Users can implement or
//!   use provided tokenizers.
//!
//! ## Architecture Flow
//!
//! 1. **Document Addition**: Users create documents according to the defined schema. The documents
//!    fields are tokenized, processed, and added to the current segment. See
//!    [Document](schema::document) for the structure and usage.
//!
//! 2. **Segment Creation**: Once the memory limit threshold is reached or a commit is called, the
//!    segment is written to the Directory. Documents are searchable after `commit`.
//!
//! 3. **Merging**: To optimize space and search speed, segments might be merged. This operation is
//!    performed in the background. Customize the merge behaviour via
//!    [IndexWriter::set_merge_policy].
#[cfg_attr(test, macro_use)]
extern crate serde_json;
#[macro_use]
extern crate log;

#[macro_use]
extern crate thiserror;

#[cfg(feature = "mmap")]
#[cfg(test)]
mod functional_test;

#[macro_use]
mod macros;
mod future_result;

pub mod columnar;

mod stacker;

pub mod tokenizer_api;

pub mod common;

// Re-exports
pub use crate::common::DateTime;
pub use {crate::query::grammar, time};

pub use crate::error::TantivyError;
pub use crate::future_result::FutureResult;

/// Tantivy result.
///
/// Within tantivy, please avoid importing `Result` using `use crate::Result`
/// and instead, refer to this as `crate::Result<T>`.
pub type Result<T> = std::result::Result<T, TantivyError>;

mod core;
pub mod indexer;

mod bitpacker;

mod sstable;

#[allow(unused_doc_comments)]
pub mod error;
pub mod tokenizer;

pub mod collector;
pub mod columnfield;
pub mod directory;
pub mod fieldnorm;
pub mod index;
pub mod positions;
pub mod postings;
pub mod roworder;

/// Module containing the different query implementations.
pub mod query;
pub mod schema;
pub mod space_usage;
pub mod store;
pub mod termdict;

mod reader;

pub use self::reader::{IndexReader, IndexReaderBuilder, ReloadPolicy, Warmer};
pub mod snippet;

mod docset;
use std::{fmt, sync::LazyLock};

pub use crate::common::{f64_to_u64, i64_to_u64, u64_to_f64, u64_to_i64, HasLen};
pub use census::{Inventory, TrackedObject};
use serde::{Deserialize, Serialize};

pub use self::docset::{DocSet, COLLECT_BLOCK_BUFFER_LEN, TERMINATED};
#[doc(hidden)]
pub use crate::core::json_utils;
pub use crate::core::{Executor, Searcher, SearcherGeneration};
pub use crate::directory::Directory;
pub use crate::index::{
    Index, IndexBuilder, IndexMeta, IndexSettings, IndexSortByField, InvertedIndexReader, Order,
    Segment, SegmentMeta, SegmentReader,
};
pub use crate::indexer::{IndexWriter, SingleSegmentIndexWriter};
pub use crate::schema::{Document, TantivyDocument, Term};

/// Index format version.
const INDEX_FORMAT_VERSION: u32 = 6;
/// Oldest index format version this tantivy version can read.
const INDEX_FORMAT_OLDEST_SUPPORTED_VERSION: u32 = 4;

/// Structure version for the index.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    major: u32,
    minor: u32,
    patch: u32,
    index_format_version: u32,
}

impl fmt::Debug for Version {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

static VERSION: LazyLock<Version> = LazyLock::new(|| Version {
    major: env!("CARGO_PKG_VERSION_MAJOR").parse().unwrap(),
    minor: env!("CARGO_PKG_VERSION_MINOR").parse().unwrap(),
    patch: env!("CARGO_PKG_VERSION_PATCH").parse().unwrap(),
    index_format_version: INDEX_FORMAT_VERSION,
});

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "tantivy v{}.{}.{}, index_format v{}",
            self.major, self.minor, self.patch, self.index_format_version
        )
    }
}

static VERSION_STRING: LazyLock<String> = LazyLock::new(|| VERSION.to_string());

/// Expose the current version of tantivy as found in Cargo.toml during compilation.
/// eg. "0.11.0" as well as the compression scheme used in the docstore.
pub fn version() -> &'static Version {
    &VERSION
}

/// Exposes the complete version of tantivy as found in Cargo.toml during compilation as a string.
/// eg. "tantivy v0.11.0, index_format v1, store_compression: lz4".
pub fn version_string() -> &'static str {
    VERSION_STRING.as_str()
}

/// Defines tantivy's merging strategy
pub mod merge_policy {
    pub use crate::indexer::{
        DefaultMergePolicy, LogMergePolicy, MergeCandidate, MergePolicy, NoMergePolicy,
    };
}

/// A `u32` identifying a document within a segment.
/// Documents have their `DocId` assigned incrementally,
/// as they are added in the segment.
///
/// At most, a segment can contain 2^31 documents.
pub type DocId = u32;

/// A u64 assigned to every operation incrementally
///
/// All operations modifying the index receives an monotonic Opstamp.
/// The resulting state of the index is consistent with the opstamp ordering.
///
/// For instance, a commit with opstamp `32_423` will reflect all Add and Delete operations
/// with an opstamp `<= 32_423`. A delete operation with opstamp n will no affect a document added
/// with opstamp `n+1`.
pub type Opstamp = u64;

/// A Score that represents the relevance of the document to the query
///
/// This is modelled internally as a `f32`. The larger the number, the more relevant
/// the document to the search query.
pub type Score = f32;

/// A `SegmentOrdinal` identifies a segment, within a `Searcher` or `Merger`.
pub type SegmentOrdinal = u32;

impl DocAddress {
    /// Creates a new DocAddress from the segment/docId pair.
    pub fn new(segment_ord: SegmentOrdinal, doc_id: DocId) -> DocAddress {
        DocAddress {
            segment_ord,
            doc_id,
        }
    }
}

/// `DocAddress` contains all the necessary information
/// to identify a document given a `Searcher` object.
///
/// It consists of an id identifying its segment, and
/// a segment-local `DocId`.
///
/// The id used for the segment is actually an ordinal
/// in the list of `Segment`s held by a `Searcher`.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    bincode::Encode,
    bincode::Decode,
)]
pub struct DocAddress {
    /// The segment ordinal id that identifies the segment
    /// hosting the document in the `Searcher` it is called from.
    pub segment_ord: SegmentOrdinal,
    /// The segment-local `DocId`.
    pub doc_id: DocId,
}

#[macro_export]
/// Enable fail_point if feature is enabled.
macro_rules! fail_point {
    ($name:expr) => {{
        #[cfg(feature = "failpoints")]
        {
            fail::eval($name, |_| {
                panic!("Return is not supported for the fail point \"{}\"", $name);
            });
        }
    }};
    ($name:expr, $e:expr) => {{
        #[cfg(feature = "failpoints")]
        {
            if let Some(res) = fail::eval($name, $e) {
                return res;
            }
        }
    }};
    ($name:expr, $cond:expr, $e:expr) => {{
        #[cfg(feature = "failpoints")]
        {
            if $cond {
                fail::fail_point!($name, $e);
            }
        }
    }};
}

#[cfg(test)]
pub mod tests {
    use crate::common::{BinarySerializable, FixedSize};
    use crate::query::grammar::{UserInputAst, UserInputLeaf, UserInputLiteral};
    use rand::distributions::{Bernoulli, Uniform};
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};
    use time::OffsetDateTime;

    use crate::collector::tests::TEST_COLLECTOR_WITH_SCORE;
    use crate::docset::{DocSet, TERMINATED};
    use crate::index::SegmentReader;
    use crate::merge_policy::NoMergePolicy;
    use crate::postings::Postings;
    use crate::query::BooleanQuery;
    use crate::schema::*;
    use crate::{DateTime, DocAddress, Index, IndexWriter, ReloadPolicy};

    pub fn fixed_size_test<O: BinarySerializable + FixedSize + Default>() {
        let mut buffer = Vec::new();
        O::default().serialize(&mut buffer).unwrap();
        assert_eq!(buffer.len(), O::SIZE_IN_BYTES);
    }

    /// Checks if left and right are close one to each other.
    /// Panics if the two values are more than 0.5% apart.
    #[macro_export]
    macro_rules! assert_nearly_equals {
        ($left:expr, $right:expr) => {{
            assert_nearly_equals!($left, $right, 0.0005);
        }};
        ($left:expr, $right:expr, $epsilon:expr) => {{
            match (&$left, &$right, &$epsilon) {
                (left_val, right_val, epsilon_val) => {
                    let diff = (left_val - right_val).abs();

                    if diff > *epsilon_val {
                        panic!(
                            r#"assertion failed: `abs(left-right)>epsilon`
    left: `{:?}`,
    right: `{:?}`,
    epsilon: `{:?}`"#,
                            &*left_val, &*right_val, &*epsilon_val
                        )
                    }
                }
            }
        }};
    }

    pub fn generate_nonunique_unsorted(max_value: u32, n_elems: usize) -> Vec<u32> {
        let seed: [u8; 32] = [1; 32];
        StdRng::from_seed(seed)
            .sample_iter(&Uniform::new(0u32, max_value))
            .take(n_elems)
            .collect::<Vec<u32>>()
    }

    pub fn sample_with_seed(n: u32, ratio: f64, seed_val: u8) -> Vec<u32> {
        StdRng::from_seed([seed_val; 32])
            .sample_iter(&Bernoulli::new(ratio).unwrap())
            .take(n as usize)
            .enumerate()
            .filter_map(|(val, keep)| if keep { Some(val as u32) } else { None })
            .collect()
    }

    pub fn sample(n: u32, ratio: f64) -> Vec<u32> {
        sample_with_seed(n, ratio, 4)
    }

    #[test]
    fn test_version_string() {
        use regex::Regex;
        let regex_ptn = Regex::new(
            "tantivy v[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}\\.{0,10}, index_format v[0-9]{1,5}",
        )
        .unwrap();
        let version = super::version().to_string();
        assert!(regex_ptn.find(&version).is_some());
    }

    #[test]
    #[cfg(feature = "mmap")]
    fn test_indexing() -> crate::Result<()> {
        let mut schema_builder = Schema::builder();
        let text_field = schema_builder.add_text_field("text", TEXT);
        let schema = schema_builder.build();
        let index = Index::create_from_tempdir(schema)?;
        // writing the segment
        let mut index_writer: IndexWriter = index.writer_for_tests()?;
        {
            let doc = doc!(text_field=>"af b");
            index_writer.add_document(doc)?;
        }
        {
            let doc = doc!(text_field=>"a b c");
            index_writer.add_document(doc)?;
        }
        {
            let doc = doc!(text_field=>"a b c d");
            index_writer.add_document(doc)?;
        }
        index_writer.commit()?;
        Ok(())
    }

    #[test]
    fn test_docfreq1() -> crate::Result<()> {
        let mut schema_builder = Schema::builder();
        let text_field = schema_builder.add_text_field("text", TEXT);
        let index = Index::create_in_ram(schema_builder.build());
        let mut index_writer: IndexWriter = index.writer_for_tests()?;
        index_writer.add_document(doc!(text_field=>"a b c"))?;
        index_writer.commit()?;
        index_writer.add_document(doc!(text_field=>"a"))?;
        index_writer.add_document(doc!(text_field=>"a a"))?;
        index_writer.commit()?;
        index_writer.add_document(doc!(text_field=>"c"))?;
        index_writer.commit()?;
        let reader = index.reader()?;
        let searcher = reader.searcher();
        let term_a = Term::from_field_text(text_field, "a");
        assert_eq!(searcher.doc_freq(&term_a)?, 3);
        let term_b = Term::from_field_text(text_field, "b");
        assert_eq!(searcher.doc_freq(&term_b)?, 1);
        let term_c = Term::from_field_text(text_field, "c");
        assert_eq!(searcher.doc_freq(&term_c)?, 2);
        let term_d = Term::from_field_text(text_field, "d");
        assert_eq!(searcher.doc_freq(&term_d)?, 0);
        Ok(())
    }

    #[test]
    fn test_fieldnorm_no_docs_with_field() -> crate::Result<()> {
        let mut schema_builder = Schema::builder();
        let title_field = schema_builder.add_text_field("title", TEXT);
        let text_field = schema_builder.add_text_field("text", TEXT);
        let index = Index::create_in_ram(schema_builder.build());
        let mut index_writer: IndexWriter = index.writer_for_tests()?;
        index_writer.add_document(doc!(text_field=>"a b c"))?;
        index_writer.commit()?;
        let index_reader = index.reader()?;
        let searcher = index_reader.searcher();
        let reader = searcher.segment_reader(0);
        {
            let fieldnorm_reader = reader.get_fieldnorms_reader(text_field)?;
            assert_eq!(fieldnorm_reader.fieldnorm(0), 3);
        }
        {
            let fieldnorm_reader = reader.get_fieldnorms_reader(title_field)?;
            assert_eq!(fieldnorm_reader.fieldnorm_id(0), 0);
        }
        Ok(())
    }

    #[test]
    fn test_fieldnorm() -> crate::Result<()> {
        let mut schema_builder = Schema::builder();
        let text_field = schema_builder.add_text_field("text", TEXT);
        let index = Index::create_in_ram(schema_builder.build());
        let mut index_writer: IndexWriter = index.writer_for_tests()?;
        index_writer.add_document(doc!(text_field=>"a b c"))?;
        index_writer.add_document(doc!())?;
        index_writer.add_document(doc!(text_field=>"a b"))?;
        index_writer.commit()?;
        let reader = index.reader()?;
        let searcher = reader.searcher();
        let segment_reader: &SegmentReader = searcher.segment_reader(0);
        let fieldnorms_reader = segment_reader.get_fieldnorms_reader(text_field)?;
        assert_eq!(fieldnorms_reader.fieldnorm(0), 3);
        assert_eq!(fieldnorms_reader.fieldnorm(1), 0);
        assert_eq!(fieldnorms_reader.fieldnorm(2), 2);
        Ok(())
    }

    #[test]
    fn test_indexed_u64() -> crate::Result<()> {
        let mut schema_builder = Schema::builder();
        let field = schema_builder.add_u64_field("value", INDEXED);
        let schema = schema_builder.build();

        let index = Index::create_in_ram(schema);
        let mut index_writer: IndexWriter = index.writer_for_tests()?;
        index_writer.add_document(doc!(field=>1u64))?;
        index_writer.commit()?;
        let reader = index.reader()?;
        let searcher = reader.searcher();
        let term = Term::from_field_u64(field, 1u64);
        let mut postings = searcher
            .segment_reader(0)
            .inverted_index(term.field())?
            .read_postings(&term, IndexRecordOption::Basic)?
            .unwrap();
        assert_eq!(postings.doc(), 0);
        assert_eq!(postings.advance(), TERMINATED);
        Ok(())
    }

    #[test]
    fn test_indexed_i64() -> crate::Result<()> {
        let mut schema_builder = Schema::builder();
        let value_field = schema_builder.add_i64_field("value", INDEXED);
        let schema = schema_builder.build();

        let index = Index::create_in_ram(schema);
        let mut index_writer: IndexWriter = index.writer_for_tests()?;
        let negative_val = -1i64;
        index_writer.add_document(doc!(value_field => negative_val))?;
        index_writer.commit()?;
        let reader = index.reader()?;
        let searcher = reader.searcher();
        let term = Term::from_field_i64(value_field, negative_val);
        let mut postings = searcher
            .segment_reader(0)
            .inverted_index(term.field())?
            .read_postings(&term, IndexRecordOption::Basic)?
            .unwrap();
        assert_eq!(postings.doc(), 0);
        assert_eq!(postings.advance(), TERMINATED);
        Ok(())
    }

    #[test]
    fn test_indexed_f64() -> crate::Result<()> {
        let mut schema_builder = Schema::builder();
        let value_field = schema_builder.add_f64_field("value", INDEXED);
        let schema = schema_builder.build();

        let index = Index::create_in_ram(schema);
        let mut index_writer: IndexWriter = index.writer_for_tests()?;
        let val = std::f64::consts::PI;
        index_writer.add_document(doc!(value_field => val))?;
        index_writer.commit()?;
        let reader = index.reader()?;
        let searcher = reader.searcher();
        let term = Term::from_field_f64(value_field, val);
        let mut postings = searcher
            .segment_reader(0)
            .inverted_index(term.field())?
            .read_postings(&term, IndexRecordOption::Basic)?
            .unwrap();
        assert_eq!(postings.doc(), 0);
        assert_eq!(postings.advance(), TERMINATED);
        Ok(())
    }

    #[test]
    fn test_indexedfield_not_in_documents() -> crate::Result<()> {
        let mut schema_builder = Schema::builder();
        let text_field = schema_builder.add_text_field("text", TEXT);
        let absent_field = schema_builder.add_text_field("absent_text", TEXT);
        let schema = schema_builder.build();
        let index = Index::create_in_ram(schema);
        let mut index_writer: IndexWriter = index.writer_for_tests()?;
        index_writer.add_document(doc!(text_field=>"a"))?;
        assert!(index_writer.commit().is_ok());
        let reader = index.reader()?;
        let searcher = reader.searcher();
        let segment_reader = searcher.segment_reader(0);
        let inverted_index = segment_reader.inverted_index(absent_field)?;
        assert_eq!(inverted_index.terms().num_terms(), 0);
        Ok(())
    }

    #[test]
    fn test_termfreq() -> crate::Result<()> {
        let mut schema_builder = Schema::builder();
        let text_field = schema_builder.add_text_field("text", TEXT);
        let schema = schema_builder.build();
        let index = Index::create_in_ram(schema);
        {
            // writing the segment
            let mut index_writer: IndexWriter = index.writer_for_tests()?;
            index_writer.add_document(doc!(text_field=>"af af af bc bc"))?;
            index_writer.commit()?;
        }
        {
            let index_reader = index.reader()?;
            let searcher = index_reader.searcher();
            let reader = searcher.segment_reader(0);
            let inverted_index = reader.inverted_index(text_field)?;
            let term_abcd = Term::from_field_text(text_field, "abcd");
            assert!(inverted_index
                .read_postings(&term_abcd, IndexRecordOption::WithFreqsAndPositions)?
                .is_none());
            let term_af = Term::from_field_text(text_field, "af");
            let mut postings = inverted_index
                .read_postings(&term_af, IndexRecordOption::WithFreqsAndPositions)?
                .unwrap();
            assert_eq!(postings.doc(), 0);
            assert_eq!(postings.term_freq(), 3);
            assert_eq!(postings.advance(), TERMINATED);
        }
        Ok(())
    }

    #[test]
    fn test_searcher_1() -> crate::Result<()> {
        let mut schema_builder = Schema::builder();
        let text_field = schema_builder.add_text_field("text", TEXT);
        let schema = schema_builder.build();
        let index = Index::create_in_ram(schema);
        let reader = index.reader()?;
        // writing the segment
        let mut index_writer: IndexWriter = index.writer_for_tests()?;
        index_writer.add_document(doc!(text_field=>"af af af b"))?;
        index_writer.add_document(doc!(text_field=>"a b c"))?;
        index_writer.add_document(doc!(text_field=>"a b c d"))?;
        index_writer.commit()?;

        reader.reload()?;
        let searcher = reader.searcher();
        let get_doc_ids = |terms: Vec<Term>| {
            let query = BooleanQuery::new_multiterms_query(terms);
            searcher
                .search(&query, &TEST_COLLECTOR_WITH_SCORE)
                .map(|topdocs| topdocs.docs().to_vec())
        };
        assert_eq!(
            get_doc_ids(vec![Term::from_field_text(text_field, "a")])?,
            vec![DocAddress::new(0, 1), DocAddress::new(0, 2)]
        );
        assert_eq!(
            get_doc_ids(vec![Term::from_field_text(text_field, "af")])?,
            vec![DocAddress::new(0, 0)]
        );
        assert_eq!(
            get_doc_ids(vec![Term::from_field_text(text_field, "b")])?,
            vec![
                DocAddress::new(0, 0),
                DocAddress::new(0, 1),
                DocAddress::new(0, 2)
            ]
        );
        assert_eq!(
            get_doc_ids(vec![Term::from_field_text(text_field, "c")])?,
            vec![DocAddress::new(0, 1), DocAddress::new(0, 2)]
        );
        assert_eq!(
            get_doc_ids(vec![Term::from_field_text(text_field, "d")])?,
            vec![DocAddress::new(0, 2)]
        );
        assert_eq!(
            get_doc_ids(vec![
                Term::from_field_text(text_field, "b"),
                Term::from_field_text(text_field, "a"),
            ])?,
            vec![
                DocAddress::new(0, 0),
                DocAddress::new(0, 1),
                DocAddress::new(0, 2)
            ]
        );
        Ok(())
    }

    #[test]
    fn test_searcher_2() -> crate::Result<()> {
        let mut schema_builder = Schema::builder();
        let text_field = schema_builder.add_text_field("text", TEXT);
        let schema = schema_builder.build();
        let index = Index::create_in_ram(schema);
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        assert_eq!(reader.searcher().num_docs(), 0u64);
        // writing the segment
        let mut index_writer: IndexWriter = index.writer_for_tests()?;
        index_writer.add_document(doc!(text_field=>"af b"))?;
        index_writer.add_document(doc!(text_field=>"a b c"))?;
        index_writer.add_document(doc!(text_field=>"a b c d"))?;
        index_writer.commit()?;
        reader.reload()?;
        assert_eq!(reader.searcher().num_docs(), 3u64);
        Ok(())
    }

    #[test]
    fn test_searcher_on_json_field_with_type_inference() {
        // When indexing and searching a json value, we infer its type.
        // This tests aims to check the type infereence is consistent between indexing and search.
        // Inference order is date, i64, u64, f64, bool.
        let mut schema_builder = Schema::builder();
        let json_field = schema_builder.add_json_field("json", STORED | TEXT);
        let schema = schema_builder.build();
        let json_val: serde_json::Value = serde_json::from_str(
            r#"{
            "signed": 2,
            "float": 2.0,
            "unsigned": 10000000000000,
            "date": "1985-04-12T23:20:50.52Z",
            "bool": true
        }"#,
        )
        .unwrap();
        let doc = doc!(json_field=>json_val);
        let index = Index::create_in_ram(schema);
        let mut writer = index.writer_for_tests().unwrap();
        writer.add_document(doc).unwrap();
        writer.commit().unwrap();
        let reader = index.reader().unwrap();
        let searcher = reader.searcher();
        let get_doc_ids = |user_input_literal: UserInputLiteral| {
            let query_parser = crate::query::QueryParser::for_index(&index, Vec::new());
            let query = query_parser
                .build_query_from_user_input_ast(UserInputAst::from(UserInputLeaf::Literal(
                    user_input_literal,
                )))
                .unwrap();
            searcher
                .search(&query, &TEST_COLLECTOR_WITH_SCORE)
                .map(|topdocs| topdocs.docs().to_vec())
                .unwrap()
        };
        {
            let user_input_literal = UserInputLiteral {
                field_name: Some("json.signed".to_string()),
                phrase: "2".to_string(),
                delimiter: crate::query::grammar::Delimiter::None,
                slop: 0,
                prefix: false,
            };
            assert_eq!(get_doc_ids(user_input_literal), vec![DocAddress::new(0, 0)]);
        }
        {
            let user_input_literal = UserInputLiteral {
                field_name: Some("json.float".to_string()),
                phrase: "2.0".to_string(),
                delimiter: crate::query::grammar::Delimiter::None,
                slop: 0,
                prefix: false,
            };
            assert_eq!(get_doc_ids(user_input_literal), vec![DocAddress::new(0, 0)]);
        }
        {
            let user_input_literal = UserInputLiteral {
                field_name: Some("json.date".to_string()),
                phrase: "1985-04-12T23:20:50.52Z".to_string(),
                delimiter: crate::query::grammar::Delimiter::None,
                slop: 0,
                prefix: false,
            };
            assert_eq!(get_doc_ids(user_input_literal), vec![DocAddress::new(0, 0)]);
        }
        {
            let user_input_literal = UserInputLiteral {
                field_name: Some("json.unsigned".to_string()),
                phrase: "10000000000000".to_string(),
                delimiter: crate::query::grammar::Delimiter::None,
                slop: 0,
                prefix: false,
            };
            assert_eq!(get_doc_ids(user_input_literal), vec![DocAddress::new(0, 0)]);
        }
        {
            let user_input_literal = UserInputLiteral {
                field_name: Some("json.bool".to_string()),
                phrase: "true".to_string(),
                delimiter: crate::query::grammar::Delimiter::None,
                slop: 0,
                prefix: false,
            };
            assert_eq!(get_doc_ids(user_input_literal), vec![DocAddress::new(0, 0)]);
        }
    }

    #[test]
    fn test_doc_macro() {
        let mut schema_builder = Schema::builder();
        let text_field = schema_builder.add_text_field("text", TEXT);
        let other_text_field = schema_builder.add_text_field("text2", TEXT);
        let document = doc!(text_field => "tantivy",
                            text_field => "some other value",
                            other_text_field => "short");
        assert_eq!(document.len(), 3);
        let values: Vec<OwnedValue> = document.get_all(text_field).map(OwnedValue::from).collect();
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].as_ref().as_str(), Some("tantivy"));
        assert_eq!(values[1].as_ref().as_str(), Some("some other value"));
        let values: Vec<OwnedValue> = document
            .get_all(other_text_field)
            .map(OwnedValue::from)
            .collect();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].as_ref().as_str(), Some("short"));
    }

    #[test]
    fn test_wrong_column_field_type() -> crate::Result<()> {
        let mut schema_builder = Schema::builder();
        let column_field_unsigned = schema_builder.add_u64_field("unsigned", COLUMN);
        let column_field_signed = schema_builder.add_i64_field("signed", COLUMN);
        let column_field_float = schema_builder.add_f64_field("float", COLUMN);
        schema_builder.add_text_field("text", TEXT);
        schema_builder.add_u64_field("stored_int", STORED);
        let schema = schema_builder.build();

        let index = Index::create_in_ram(schema);
        let mut index_writer: IndexWriter = index.writer_for_tests()?;
        {
            let document = doc!(column_field_unsigned => 4u64, column_field_signed=>4i64, column_field_float=>4f64);
            index_writer.add_document(document)?;
            index_writer.commit()?;
        }
        let reader = index.reader()?;
        let searcher = reader.searcher();
        let segment_reader: &SegmentReader = searcher.segment_reader(0);
        {
            let column_field_reader_res = segment_reader.column_fields().u64("text");
            assert!(column_field_reader_res.is_err());
        }
        {
            let column_field_reader_opt = segment_reader.column_fields().u64("stored_int");
            assert!(column_field_reader_opt.is_err());
        }
        {
            let column_field_reader_opt = segment_reader.column_fields().u64("signed");
            assert!(column_field_reader_opt.is_err());
        }
        {
            let column_field_reader_opt = segment_reader.column_fields().u64("float");
            assert!(column_field_reader_opt.is_err());
        }
        {
            let column_field_reader_opt = segment_reader.column_fields().u64("unsigned");
            assert!(column_field_reader_opt.is_ok());
            let column_field_reader = column_field_reader_opt.unwrap();
            assert_eq!(column_field_reader.first(0), Some(4u64))
        }

        {
            let column_field_reader_res = segment_reader.column_fields().i64("signed");
            assert!(column_field_reader_res.is_ok());
            let column_field_reader = column_field_reader_res.unwrap();
            assert_eq!(column_field_reader.first(0), Some(4i64))
        }

        {
            let column_field_reader_res = segment_reader.column_fields().f64("float");
            assert!(column_field_reader_res.is_ok());
            let column_field_reader = column_field_reader_res.unwrap();
            assert_eq!(column_field_reader.first(0), Some(4f64))
        }
        Ok(())
    }

    #[test]
    fn test_validate_checksum() -> crate::Result<()> {
        let index_path = tempfile::tempdir().expect("dir");
        let mut builder = Schema::builder();
        let body = builder.add_text_field("body", TEXT | STORED);
        let schema = builder.build();
        let index = Index::create_in_dir(&index_path, schema)?;
        let mut writer: IndexWriter = index.writer(50_000_000)?;
        writer.set_merge_policy(Box::new(NoMergePolicy));
        for _ in 0..5000 {
            writer.add_document(doc!(body => "foo"))?;
            writer.add_document(doc!(body => "boo"))?;
        }
        writer.commit()?;
        assert!(index.validate_checksum()?.is_empty());

        let segment_ids = index.searchable_segment_ids()?;
        writer.merge(&segment_ids).wait()?;
        assert!(index.validate_checksum()?.is_empty());
        Ok(())
    }

    #[test]
    fn test_datetime() {
        let now = OffsetDateTime::now_utc();

        let dt = DateTime::from_utc(now).into_utc();
        assert_eq!(dt.to_ordinal_date(), now.to_ordinal_date());
        assert_eq!(dt.to_hms_micro(), now.to_hms_micro());
        // We store nanosecond level precision.
        assert_eq!(dt.nanosecond(), now.nanosecond());

        let dt = DateTime::from_timestamp_secs(now.unix_timestamp()).into_utc();
        assert_eq!(dt.to_ordinal_date(), now.to_ordinal_date());
        assert_eq!(dt.to_hms(), now.to_hms());
        // Constructed from a second precision.
        assert_ne!(dt.to_hms_micro(), now.to_hms_micro());

        let dt =
            DateTime::from_timestamp_micros((now.unix_timestamp_nanos() / 1_000) as i64).into_utc();
        assert_eq!(dt.to_ordinal_date(), now.to_ordinal_date());
        assert_eq!(dt.to_hms_micro(), now.to_hms_micro());

        let dt_from_ts_nanos =
            OffsetDateTime::from_unix_timestamp_nanos(1492432621123456789).unwrap();
        let offset_dt = DateTime::from_utc(dt_from_ts_nanos).into_utc();
        assert_eq!(
            dt_from_ts_nanos.to_ordinal_date(),
            offset_dt.to_ordinal_date()
        );
        assert_eq!(dt_from_ts_nanos.to_hms_micro(), offset_dt.to_hms_micro());
    }
}
