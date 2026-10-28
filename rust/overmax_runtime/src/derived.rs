//! 파생 상태: 감지 상태와 기록에서 다시 계산되는 값 (추천 목록).
//!
//! 감지 결과뿐 아니라 업로드·V-Archive 조회·Steam 세션 전환 등 여러 경로에서
//! 다시 계산되므로 허브 관찰자가 아닌 호출자가 필요할 때 부른다.

use overmax_core::GameSessionState;
use overmax_data::service::recommend_provider_fetch;
use overmax_data::{
    ProviderCacheReader, RecommendContext, RecommendResult, RecommendStrategy, Recommender,
    Settings,
};
use std::path::Path;

pub fn recommend_for_state(
    recommender: &Recommender,
    settings: &Settings,
    root: &Path,
    v_id: Option<String>,
    state: &GameSessionState,
) -> RecommendResult {
    let Some(ctx) = &state.context else {
        return RecommendResult::empty();
    };
    let rec_settings = settings.recommend();
    let smart_recommend = rec_settings.smart_recommend;
    let target_rate = rec_settings.target_rate;
    let strategy = RecommendStrategy::from_smart_flag(smart_recommend);
    let rec_ctx = RecommendContext {
        song_id: ctx.song_id,
        button_mode: ctx.mode,
        difficulty: ctx.diff,
        floor_range: 0.0,
        max_results: 6,
        same_mode_only: true,
        v_id,
        strategy,
        target_rate,
    };

    let provider_settings = settings.recommend_provider();
    if provider_settings.enabled {
        if let Some(url) = &provider_settings.url {
            let clean_url = url.trim();
            if !clean_url.is_empty() {
                let provider_name = provider_settings
                    .name
                    .as_deref()
                    .filter(|n| !n.is_empty())
                    .unwrap_or("external_provider");
                let cache_dir = root
                    .join("cache")
                    .join("recommend_provider")
                    .join(provider_name);

                let manifest = recommend_provider_fetch::get_cached_manifest(clean_url);

                let reader = ProviderCacheReader::new(
                    provider_name,
                    provider_name,
                    cache_dir.clone(),
                    manifest.vary,
                    std::time::Duration::from_secs(manifest.ttl_sec),
                );

                let clean_url_clone = clean_url.to_string();
                let rec_ctx_clone = rec_ctx.clone();
                let cache_dir_clone = cache_dir.clone();
                let cache_key = reader.cache_key(&rec_ctx);
                let cache_path = cache_dir_clone.join(format!("{}.json", cache_key));

                let should_fetch = match std::fs::metadata(&cache_path) {
                    Ok(meta) => meta
                        .modified()
                        .ok()
                        .and_then(|m| m.elapsed().ok())
                        .map(|e| e.as_secs() > 10)
                        .unwrap_or(true),
                    Err(_) => true,
                };

                if should_fetch {
                    std::thread::spawn(move || {
                        let manifest =
                            recommend_provider_fetch::fetch_manifest_blocking(&clean_url_clone);
                        let _ = recommend_provider_fetch::fetch_recommend_blocking(
                            &clean_url_clone,
                            &manifest,
                            &rec_ctx_clone,
                            &cache_path,
                        );
                    });
                }

                let composite = recommender.clone().with_provider(reader);
                return composite.recommend_panel(&rec_ctx).as_legacy_result();
            }
        }
    }

    recommender.recommend_panel(&rec_ctx).as_legacy_result()
}
