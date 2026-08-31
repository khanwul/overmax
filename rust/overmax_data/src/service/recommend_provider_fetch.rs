//! Recommendation Provider Fetcher facade.
//!
//! Re-exports from `crate::gateway::recommend_provider` for backward compatibility.

pub use crate::gateway::recommend_provider::{
    fetch_manifest_blocking, fetch_recommend_blocking, get_cached_manifest,
    test_provider_connection_blocking, ProviderManifest, RECOMMEND_PROTOCOL_ID,
};
