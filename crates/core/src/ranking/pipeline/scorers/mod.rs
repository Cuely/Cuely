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

pub mod initial;
pub mod reranker;

use std::sync::Arc;

pub use initial::Initial;
pub use reranker::ReRanker;

use crate::{
    enum_map::EnumMap,
    ranking::{models::lambdamart::LambdaMART, Signal, SignalCoefficient, SignalScore},
    searcher::SearchQuery,
};

use super::RankableWebpage;

pub trait Scorer<T: RankableWebpage>: Send + Sync {
    fn score(&self, webpages: &mut [T]);
    fn set_query_info(&mut self, _query: &SearchQuery) {}
}

pub struct IdentityScorer;

impl<T: RankableWebpage> Scorer<T> for IdentityScorer {
    fn score(&self, _webpages: &mut [T]) {}
}

fn calculate_score(
    model: &Option<Arc<LambdaMART>>,
    signal_coefficients: &Option<SignalCoefficient>,
    signals: &EnumMap<Signal, SignalScore>,
) -> f64 {
    let lambda_score = match model {
        Some(model) => match signal_coefficients {
            Some(coefficients) => match coefficients.get(&Signal::LambdaMART) {
                Some(coeff) => {
                    if coeff == 0.0 {
                        signals
                            .values()
                            .map(|score| score.coefficient * score.value)
                            .sum()
                    } else {
                        coeff * model.predict(signals)
                    }
                }
                None => Signal::LambdaMART.default_coefficient() * model.predict(signals),
            },
            None => Signal::LambdaMART.default_coefficient() * model.predict(signals),
        },
        None => signals
            .values()
            .map(|score| score.coefficient * score.value)
            .sum(),
    };

    lambda_score
}
