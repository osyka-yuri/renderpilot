//! Platform-independent recommendation ordering policy.

use super::types::{RootRecommendation, RootRecommendationSource};

pub(super) fn choose_best_recommendation(
    recommendations: Vec<(usize, RootRecommendation)>,
) -> Option<RootRecommendation> {
    recommendations
        .into_iter()
        .min_by_key(|(distance, recommendation)| {
            (recommendation_priority(recommendation.source), *distance)
        })
        .map(|(_, recommendation)| recommendation)
}

const fn recommendation_priority(source: RootRecommendationSource) -> u8 {
    match source {
        RootRecommendationSource::LauncherManifest => 0,
        RootRecommendationSource::EngineDistributionRoot => 1,
        RootRecommendationSource::RootExecutable => 2,
        RootRecommendationSource::ComponentContext => 3,
    }
}
