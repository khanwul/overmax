pub mod composite;
pub mod local;
pub mod scoring;
pub mod strategy;
pub mod types;

#[cfg(test)]
mod tests;

pub use composite::{CompositeRecommender, ProviderCacheReader, Recommender};
pub use local::LocalFloorRecommender;
pub use scoring::{
    calculate_performance_rating, is_base_bundle_dlc, SessionPlayInfo, SessionTrend,
    SessionTrendState,
};
pub use types::{
    FloorCacheKey, FloorRateSummary, LocalRecommendFooter, RecommendBundle, RecommendContext,
    RecommendEntry, RecommendPanel, RecommendReason, RecommendReasonKind, RecommendResult,
    RecommendStrategy, RecommendationSource, SourceStatus, VaryDim,
};
