//! Zero-cost compile-time i18n lookup system with single unified top-level t! macro.

use serde_json::Value;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Locale {
    #[default]
    Ko,
    En,
}

static CURRENT_LOCALE: AtomicU8 = AtomicU8::new(0);

pub fn set_locale(locale: Locale) {
    let code = match locale {
        Locale::Ko => 0,
        Locale::En => 1,
    };
    CURRENT_LOCALE.store(code, Ordering::Relaxed);
}

pub fn current_locale() -> Locale {
    match CURRENT_LOCALE.load(Ordering::Relaxed) {
        1 => Locale::En,
        _ => Locale::Ko,
    }
}

/// Reads the top-level `"language"` key (`"ko"`/`"en"`) from merged settings JSON.
pub fn set_locale_from_settings(settings: &Value) {
    let locale = match settings.get("language").and_then(Value::as_str) {
        Some("en") => Locale::En,
        _ => Locale::Ko,
    };
    set_locale(locale);
}

/// Zero-cost, zero-reallocation macro-based i18n dispatcher.
/// Selects translation based on current locale without string allocations.
#[macro_export]
macro_rules! t_select {
    (Ko => $ko:expr, En => $en:expr) => {
        match $crate::ui::i18n::current_locale() {
            $crate::ui::i18n::Locale::Ko => $ko,
            $crate::ui::i18n::Locale::En => $en,
        }
    };
}

/// Single top-level SSOT i18n macro for static keys, dynamic formatters, and domain metas.
#[macro_export]
macro_rules! t {
    // 1) Domain Meta Direct Matchers
    (gold = $meta:expr) => {
        match $meta {
            overmax_data::community::sheet_meta::GoldMeta::HalfRandom => $crate::t!("gold-half-random"),
            overmax_data::community::sheet_meta::GoldMeta::MaxRandom => $crate::t!("gold-max-random"),
            overmax_data::community::sheet_meta::GoldMeta::Random => $crate::t!("gold-random"),
            overmax_data::community::sheet_meta::GoldMeta::None => "",
        }
    };
    (assist = $meta:expr) => {
        match $meta {
            overmax_data::community::sheet_meta::AssistMeta::Used => $crate::t!("assist-used"),
            overmax_data::community::sheet_meta::AssistMeta::Caution => $crate::t!("assist-caution"),
            overmax_data::community::sheet_meta::AssistMeta::NotUsed => $crate::t!("assist-not-used"),
            overmax_data::community::sheet_meta::AssistMeta::None => "",
        }
    };

    // 2) Dynamic Format Keys (Arg-based formatting)
    ("candidate-count", n = $n:expr) => {
        $crate::t_select!(
            Ko => format!("후보 {}건", $n),
            En => format!("{} candidates", $n)
        )
    };
    ("sys-place-achieved", mode = $mode:expr, rank = $rank:expr) => {
        $crate::t_select!(
            Ko => format!("{} TOP {}위 달성!", $mode, $rank),
            En => format!("Achieved TOP {} in {}!", $rank, $mode)
        )
    };
    ("sys-upload-cache-error", error = $err:expr) => {
        $crate::t_select!(
            Ko => format!("업로드 OK, 캐시 갱신 오류: {}", $err),
            En => format!("Upload OK, cache error: {}", $err)
        )
    };
    ("sys-update-prompt-dialog", current = $current:expr, latest = $latest:expr) => {
        $crate::t_select!(
            Ko => format!("새 앱 업데이트가 있습니다.\n\n현재 버전: {}\n최신 버전: {}\n\n지금 업데이트를 진행할까요?", $current, $latest),
            En => format!("A new app update is available.\n\nCurrent version: {}\nLatest version: {}\n\nWould you like to update now?", $current, $latest)
        )
    };
    ("sys-update-error-dialog", error = $err:expr) => {
        $crate::t_select!(
            Ko => format!("자동 패치가 완료되지 않았습니다.\n\n사유: {}", $err),
            En => format!("The automatic update did not complete.\n\nReason: {}", $err)
        )
    };
    ("sys-api-ok-cache-failed", error = $err:expr) => {
        $crate::t_select!(
            Ko => format!("API 조회 OK, 캐시 병합 실패: {}", $err),
            En => format!("API fetch OK, cache merge failed: {}", $err)
        )
    };
    ("sys-api-and-fallback-failed", api_error = $api_err:expr, fallback_error = $fb_err:expr) => {
        $crate::t_select!(
            Ko => format!("API 실패 ({}), 폴백 캐시 갱신 실패: {}", $api_err, $fb_err),
            En => format!("API failed ({}), fallback cache update failed: {}", $api_err, $fb_err)
        )
    };
    ("sys-fallback-cache-failed", error = $err:expr) => {
        $crate::t_select!(
            Ko => format!("폴백 캐시 갱신 실패: {}", $err),
            En => format!("Fallback cache update failed: {}", $err)
        )
    };
    ("settings-eg-hint", domain = $domain:expr) => {
        $crate::t_select!(
            Ko => format!("예: {}", $domain),
            En => format!("e.g. {}", $domain)
        )
    };
    ("sync-filter-btn", icon = $icon:expr) => {
        $crate::t_select!(
            Ko => format!("{} 🔍 필터", $icon),
            En => format!("{} 🔍 Filter", $icon)
        )
    };
    ("sync-level-range", min = $min:expr, max = $max:expr) => {
        $crate::t_select!(
            Ko => format!("난이도 ({} ~ {})", $min, $max),
            En => format!("Level ({} ~ {})", $min, $max)
        )
    };
    ("rec-patterns-count", has = $has:expr, total = $total:expr) => {
        $crate::t_select!(
            Ko => format!("{}/{}곡", $has, $total),
            En => format!("{}/{} patterns", $has, $total)
        )
    };
    ("overlay-gold-rec-meta", val = $val:expr) => {
        $crate::t_select!(
            Ko => format!("황배:{}", $val),
            En => format!("Recommendation:{}", $val)
        )
    };
    ("overlay-assist-key-meta", val = $val:expr) => {
        $crate::t_select!(
            Ko => format!("보조:{}", $val),
            En => format!("Assist Key:{}", $val)
        )
    };
    ("status-varchive-failed-toast", error = $err:expr) => {
        $crate::t_select!(
            Ko => format!("V-Archive 동기화 실패: {}", $err),
            En => format!("V-Archive sync failed: {}", $err)
        )
    };
    ("sys-shortcut-create-success", path = $path:expr) => {
        $crate::t_select!(
            Ko => format!("앱 메뉴 바로가기를 생성했습니다.\n\n{}", $path),
            En => format!("App menu shortcut created successfully.\n\n{}", $path)
        )
    };
    ("sys-shortcut-create-failed", error = $err:expr) => {
        $crate::t_select!(
            Ko => format!("바로가기를 생성하지 못했습니다.\n\n{}", $err),
            En => format!("Failed to create shortcut.\n\n{}", $err)
        )
    };
    ("sys-upload-msg-with-rank", message = $msg:expr, rank_msg = $rank:expr) => {
        $crate::t_select!(
            Ko => format!("{} ({})", $msg, $rank),
            En => format!("{} ({})", $msg, $rank)
        )
    };

    // 3) Static Keys Direct Matchers (Multi-line formatted per key for legibility & DX)
    ("status-account-path-missing") => {
        $crate::t_select!(
            Ko => "account.txt 경로 설정 필요",
            En => "account.txt path required"
        )
    };
    ("status-account-parse-failed") => {
        $crate::t_select!(
            Ko => "account.txt 읽기 실패",
            En => "Failed to read account.txt"
        )
    };

    ("gold-half-random") => {
        $crate::t_select!(
            Ko => "핲랜",
            En => "Half Random"
        )
    };
    ("gold-max-random") => {
        $crate::t_select!(
            Ko => "맥랜",
            En => "Max Random"
        )
    };
    ("gold-random") => {
        $crate::t_select!(
            Ko => "랜덤",
            En => "Random"
        )
    };
    ("assist-used") => {
        $crate::t_select!(
            Ko => "사용",
            En => "Used"
        )
    };
    ("assist-caution") => {
        $crate::t_select!(
            Ko => "주의",
            En => "Caution"
        )
    };
    ("assist-not-used") => {
        $crate::t_select!(
            Ko => "미사용",
            En => "Not Used"
        )
    };

    ("settings-title") => {
        $crate::t_select!(
            Ko => "설정",
            En => "Settings"
        )
    };
    ("settings-overlay-section") => {
        $crate::t_select!(
            Ko => "오버레이 설정",
            En => "Overlay Settings"
        )
    };
    ("settings-size") => {
        $crate::t_select!(
            Ko => "크기",
            En => "Size"
        )
    };
    ("settings-opacity") => {
        $crate::t_select!(
            Ko => "투명도",
            En => "Opacity"
        )
    };
    ("settings-lite-mode") => {
        $crate::t_select!(
            Ko => "라이트모드",
            En => "Lite Mode"
        )
    };
    ("settings-enable") => {
        $crate::t_select!(
            Ko => "활성화",
            En => "Enable"
        )
    };
    ("settings-lite-mode-desc") => {
        $crate::t_select!(
            Ko => "추천 숨기기 및 레이아웃 축소",
            En => "Hide recommendations and shrink layout"
        )
    };
    ("settings-snap-position") => {
        $crate::t_select!(
            Ko => "오버레이 고정 위치",
            En => "Overlay Snap Position"
        )
    };
    ("settings-top-left") => {
        $crate::t_select!(
            Ko => "좌상단",
            En => "Top-Left"
        )
    };
    ("settings-top-right") => {
        $crate::t_select!(
            Ko => "우상단",
            En => "Top-Right"
        )
    };
    ("settings-bottom-left") => {
        $crate::t_select!(
            Ko => "좌하단",
            En => "Bottom-Left"
        )
    };
    ("settings-bottom-right") => {
        $crate::t_select!(
            Ko => "우하단",
            En => "Bottom-Right"
        )
    };
    ("settings-manual") => {
        $crate::t_select!(
            Ko => "수동",
            En => "Manual"
        )
    };
    ("settings-varchive-account") => {
        $crate::t_select!(
            Ko => "V-Archive 계정",
            En => "V-Archive Account"
        )
    };
    ("settings-link-status") => {
        $crate::t_select!(
            Ko => "연동 상태",
            En => "Link Status"
        )
    };
    ("settings-data-sync") => {
        $crate::t_select!(
            Ko => "데이터 동기화",
            En => "Data Sync"
        )
    };
    ("settings-browse") => {
        $crate::t_select!(
            Ko => "찾기",
            En => "Browse"
        )
    };
    ("settings-update-section") => {
        $crate::t_select!(
            Ko => "업데이트 설정",
            En => "Update Settings"
        )
    };
    ("settings-auto-update") => {
        $crate::t_select!(
            Ko => "자동 업데이트",
            En => "Auto Update"
        )
    };
    ("settings-use") => {
        $crate::t_select!(
            Ko => "사용",
            En => "Enable"
        )
    };
    ("settings-version-info") => {
        $crate::t_select!(
            Ko => "버전 정보",
            En => "Version"
        )
    };
    ("settings-no-steam-account") => {
        $crate::t_select!(
            Ko => "발견된 Steam 계정이 없습니다.",
            En => "No Steam account found."
        )
    };
    ("settings-general") => {
        $crate::t_select!(
            Ko => "일반",
            En => "General"
        )
    };
    ("settings-language") => {
        $crate::t_select!(
            Ko => "언어",
            En => "Language"
        )
    };
    ("settings-recommend-provider") => {
        $crate::t_select!(
            Ko => "추천 Provider",
            En => "Recommend Provider"
        )
    };
    ("settings-recommend-section") => {
        $crate::t_select!(
            Ko => "추천 설정",
            En => "Recommendation"
        )
    };
    ("settings-smart-recommend") => {
        $crate::t_select!(
            Ko => "스마트 추천",
            En => "Smart Recommendation"
        )
    };
    ("settings-smart-recommend-desc") => {
        $crate::t_select!(
            Ko => "Top-50 경계, 방치 재도전 및 세션 모멘텀을 반영해 가중 정렬하고 사유 뱃지를 표시합니다.\n비활성화 시 기존 단순 달성률 순서로 정렬됩니다.",
            En => "Weighted sorting by Top-50 boundaries, retry gap, and session momentum with reason badges.\nWhen disabled, sorts by classic achievement rate only."
        )
    };
    ("settings-use-external-provider") => {
        $crate::t_select!(
            Ko => "외부 Provider 사용",
            En => "Use External Provider"
        )
    };
    ("settings-display-name") => {
        $crate::t_select!(
            Ko => "표시 이름",
            En => "Display Name"
        )
    };
    ("settings-overlay-display") => {
        $crate::t_select!(
            Ko => "오버레이 표시",
            En => "Overlay Display"
        )
    };
    ("settings-always-show") => {
        $crate::t_select!(
            Ko => "항상 표시",
            En => "Always Show"
        )
    };
    ("settings-always-show-desc") => {
        $crate::t_select!(
            Ko => "게임 구동 중 씬 감지(Unknown) 결과와 상관없이 오버레이를 항상 표시합니다.",
            En => "Keeps the overlay visible at all times, regardless of scene detection (Unknown) results."
        )
    };
    ("settings-diagnostics") => {
        $crate::t_select!(
            Ko => "진단 및 디버그",
            En => "Diagnostics & Debug"
        )
    };
    ("settings-debug-window") => {
        $crate::t_select!(
            Ko => "디버그 창",
            En => "Debug Window"
        )
    };
    ("settings-show-debug-window") => {
        $crate::t_select!(
            Ko => "디버그 모니터링 창 표시",
            En => "Show Debug Monitoring Window"
        )
    };
    ("settings-debug-window-desc") => {
        $crate::t_select!(
            Ko => "실시간 탐지 수치 및 진단 로그를 표출하는 디버그 창을 엽니다.",
            En => "Opens a debug window showing real-time detection metrics and diagnostic logs."
        )
    };
    ("settings-screen-capture") => {
        $crate::t_select!(
            Ko => "화면 캡처 설정",
            En => "Screen Capture Settings"
        )
    };
    ("settings-protect-overlay") => {
        $crate::t_select!(
            Ko => "캡처 시 오버레이 보호",
            En => "Protect Overlay from Capture"
        )
    };
    ("settings-prevent-screen-capture") => {
        $crate::t_select!(
            Ko => "화면 캡처 방지",
            En => "Prevent Screen Capture"
        )
    };
    ("settings-protect-overlay-desc") => {
        $crate::t_select!(
            Ko => "해제 시 화면 캡쳐에 잡히는 대신, 특정 영역에 오버레이가 위치하면 곡 인식이 제대로 동작하지 않게 됩니다.",
            En => "Disabling this exposes the overlay to screen capture, but recognition may fail if the overlay covers key areas."
        )
    };
    ("settings-find-sync-candidates-btn") => {
        $crate::t_select!(
            Ko => "🔍 동기화 후보 찾기",
            En => "🔍 Find Sync Candidates"
        )
    };
    ("settings-capture-method-win") => {
        $crate::t_select!(
            Ko => "화면 캡처 설정 (Windows)",
            En => "Screen Capture Method (Windows)"
        )
    };
    ("settings-capture-mode-auto") => {
        $crate::t_select!(
            Ko => "자동 (추천)",
            En => "Auto (Recommended)"
        )
    };
    ("settings-capture-mode-dxgi") => {
        $crate::t_select!(
            Ko => "DXGI (고성능)",
            En => "DXGI (High Perf)"
        )
    };
    ("settings-capture-mode-gdi") => {
        $crate::t_select!(
            Ko => "GDI (호환성)",
            En => "GDI (Compatibility)"
        )
    };

    ("sync-title") => {
        $crate::t_select!(
            Ko => "동기화",
            En => "Sync"
        )
    };
    ("sync-desc") => {
        $crate::t_select!(
            Ko => "Steam 계정 기준으로 업로드 후보를 확인합니다.",
            En => "Checks upload candidates for the current Steam account."
        )
    };
    ("sync-scan") => {
        $crate::t_select!(
            Ko => "스캔",
            En => "Scan"
        )
    };
    ("sync-upload-candidates") => {
        $crate::t_select!(
            Ko => "업로드 후보",
            En => "Upload Candidates"
        )
    };
    ("sync-sort-by-change") => {
        $crate::t_select!(
            Ko => "변경순",
            En => "By Change"
        )
    };
    ("sync-sort-by-title") => {
        $crate::t_select!(
            Ko => "제목순",
            En => "By Title"
        )
    };
    ("sync-varchive-sync") => {
        $crate::t_select!(
            Ko => "V-Archive 동기화",
            En => "V-Archive Sync"
        )
    };
    ("sync-register") => {
        $crate::t_select!(
            Ko => "등록",
            En => "Register"
        )
    };
    ("sync-delete") => {
        $crate::t_select!(
            Ko => "삭제",
            En => "Delete"
        )
    };
    ("sync-mode") => {
        $crate::t_select!(
            Ko => "모드",
            En => "Mode"
        )
    };
    ("sync-difficulty") => {
        $crate::t_select!(
            Ko => "난이도",
            En => "Difficulty"
        )
    };
    ("sync-max-combo-only") => {
        $crate::t_select!(
            Ko => "맥스콤보 달성만",
            En => "Max Combo achieved only"
        )
    };
    ("sync-exclude-unuploaded") => {
        $crate::t_select!(
            Ko => "미업로드 제외",
            En => "Exclude not uploaded"
        )
    };
    ("sync-reason-not-registered") => {
        $crate::t_select!(
            Ko => "미등록",
            En => "Not Registered"
        )
    };
    ("sync-reset-btn") => {
        $crate::t_select!(
            Ko => "초기화 ↺",
            En => "Reset ↺"
        )
    };

    ("app-settings-window") => {
        $crate::t_select!(
            Ko => "Overmax 설정",
            En => "Overmax Settings"
        )
    };
    ("app-close") => {
        $crate::t_select!(
            Ko => "닫기",
            En => "Close"
        )
    };
    ("app-save") => {
        $crate::t_select!(
            Ko => "저장",
            En => "Save"
        )
    };
    ("Linux 앱 실행") => {
        $crate::t_select!(
            Ko => "Linux 앱 실행",
            En => "Linux 앱 실행"
        )
    };
    ("앱 메뉴") => {
        $crate::t_select!(
            Ko => "앱 메뉴",
            En => "앱 메뉴"
        )
    };
    ("바로가기 생성") => {
        $crate::t_select!(
            Ko => "바로가기 생성",
            En => "바로가기 생성"
        )
    };

    ("tray-exit") => {
        $crate::t_select!(
            Ko => "종료",
            En => "Exit"
        )
    };

    ("overlay-varchive-upload-needed") => {
        $crate::t_select!(
            Ko => "V-Archive 업로드 필요 (클릭하여 즉시 업로드)",
            En => "V-Archive upload needed (click to upload now)"
        )
    };
    ("overlay-varchive-link-needed") => {
        $crate::t_select!(
            Ko => "V-Archive 계정 연동 필요 (설정에서 account.txt 경로를 지정해주세요)",
            En => "V-Archive account link needed (set account.txt path in Settings)"
        )
    };
    ("overlay-similar-avg") => {
        $crate::t_select!(
            Ko => "유사 구간 평균",
            En => "Similar Section Average"
        )
    };
    ("overlay-keypart-focused") => {
        $crate::t_select!(
            Ko => "키파트 위주 패턴",
            En => "Key-part Focused Pattern"
        )
    };
    ("overlay-gold-rec") => {
        $crate::t_select!(
            Ko => "황배",
            En => "Recommendation"
        )
    };
    ("overlay-assist-key") => {
        $crate::t_select!(
            Ko => "보조",
            En => "Assist Key"
        )
    };
    ("overlay-no-record") => {
        $crate::t_select!(
            Ko => "기록 없음",
            En => "No Record"
        )
    };

    ("rec-detecting-pattern") => {
        $crate::t_select!(
            Ko => "패턴을 감지하는 중...",
            En => "Detecting pattern..."
        )
    };
    ("rec-no-recommendations") => {
        $crate::t_select!(
            Ko => "추천 결과 없음",
            En => "No recommendations"
        )
    };
    ("rec-patterns-suffix") => {
        $crate::t_select!(
            Ko => "개 패턴",
            En => " patterns"
        )
    };
    ("rec-select-song") => {
        $crate::t_select!(
            Ko => "곡을 선택하세요",
            En => "Please select a song"
        )
    };

    ("status-scanning") => {
        $crate::t_select!(
            Ko => "스캔 중…",
            En => "Scanning…"
        )
    };
    ("status-updated") => {
        $crate::t_select!(
            Ko => "갱신 완료",
            En => "Updated"
        )
    };
    ("status-registered") => {
        $crate::t_select!(
            Ko => "등록 완료",
            En => "Registered"
        )
    };

    ("sys-already-running") => {
        $crate::t_select!(
            Ko => "이미 Overmax가 실행 중입니다. 기존 인스턴스를 종료한 뒤 다시 실행하세요.",
            En => "Overmax is already running. Please close the existing instance and try again."
        )
    };
    ("sys-reason") => {
        $crate::t_select!(
            Ko => "사유",
            En => "Reason"
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use overmax_data::community::sheet_meta::{AssistMeta, GoldMeta};

    #[test]
    fn test_zero_cost_i18n_translation_and_locale_switch() {
        set_locale(Locale::Ko);
        assert_eq!(t!("settings-title"), "설정");
        assert_eq!(t!("candidate-count", n = 3), "후보 3건");
        assert_eq!(
            t!("sys-place-achieved", mode = "4B", rank = 5),
            "4B TOP 5위 달성!"
        );
        assert_eq!(
            t!("sys-upload-cache-error", error = "timeout"),
            "업로드 OK, 캐시 갱신 오류: timeout"
        );
        assert_eq!(t!("rec-patterns-count", has = 3, total = 5), "3/5곡");
        assert_eq!(
            t!("sys-shortcut-create-success", path = "/usr/bin/app"),
            "앱 메뉴 바로가기를 생성했습니다.\n\n/usr/bin/app"
        );
        assert_eq!(
            t!(
                "sys-upload-msg-with-rank",
                message = "성공",
                rank_msg = "4B TOP 1위 달성!"
            ),
            "성공 (4B TOP 1위 달성!)"
        );
        assert_eq!(t!(gold = GoldMeta::HalfRandom), "핲랜");
        assert_eq!(t!(assist = AssistMeta::Caution), "주의");

        set_locale(Locale::En);
        assert_eq!(t!("settings-title"), "Settings");
        assert_eq!(t!("candidate-count", n = 3), "3 candidates");
        assert_eq!(
            t!("sys-place-achieved", mode = "4B", rank = 5),
            "Achieved TOP 5 in 4B!"
        );
        assert_eq!(
            t!("sys-upload-cache-error", error = "timeout"),
            "Upload OK, cache error: timeout"
        );
        assert_eq!(t!("rec-patterns-count", has = 3, total = 5), "3/5 patterns");
        assert_eq!(
            t!("sys-shortcut-create-success", path = "/usr/bin/app"),
            "App menu shortcut created successfully.\n\n/usr/bin/app"
        );
        assert_eq!(t!(gold = GoldMeta::HalfRandom), "Half Random");
        assert_eq!(t!(assist = AssistMeta::Caution), "Caution");

        set_locale(Locale::Ko);
    }
}
