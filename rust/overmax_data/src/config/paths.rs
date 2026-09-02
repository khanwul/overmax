use std::fs;
use std::path::{Path, PathBuf};

use super::compatibility::DataCompatibility;
use super::settings::SettingsPaths;

/// 런타임 데이터 저장 모드
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeMode {
    /// 바이너리 상대 경로에 모든 데이터를 저장하는 포터블 모드
    Portable,
    /// %LOCALAPPDATA%\Overmax 또는 XDG 데이터 디렉터리에 가변 데이터를 저장하는 설치/패키지 모드
    Installed,
}

/// 데이터 경로 추상화 레이어
///
/// 번들된 읽기 전용 에셋(`bundle_dir`)과 런타임에 갱신/저장되는 사용자 데이터(`data_dir`)의
/// 위치를 추상화하고, Portable 모드와 MSIX/Installed 모드를 투명하게 지원합니다.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppPaths {
    mode: RuntimeMode,
    bundle_dir: PathBuf,
    data_dir: PathBuf,
}

impl AppPaths {
    /// 단일 루트 디렉터리를 사용하는 Portable 모드 경로 생성
    pub fn new_portable(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            mode: RuntimeMode::Portable,
            bundle_dir: root.clone(),
            data_dir: root,
        }
    }

    /// 번들 디렉터리와 데이터 디렉터리가 분리된 Installed 모드 경로 생성
    pub fn new_installed(bundle_dir: impl Into<PathBuf>, data_dir: impl Into<PathBuf>) -> Self {
        Self {
            mode: RuntimeMode::Installed,
            bundle_dir: bundle_dir.into(),
            data_dir: data_dir.into(),
        }
    }

    /// 커스텀 경로 및 모드 지정 생성
    pub fn from_custom(
        bundle_dir: impl Into<PathBuf>,
        data_dir: impl Into<PathBuf>,
        mode: RuntimeMode,
    ) -> Self {
        Self {
            mode,
            bundle_dir: bundle_dir.into(),
            data_dir: data_dir.into(),
        }
    }

    /// 현재 런타임 환경(MSIX 패키지, 환경 변수, 실행 디렉터리 권한 등)을 자동 감지하여 경로를 확정합니다.
    pub fn resolve() -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()));
        let current_dir = std::env::current_dir().ok();
        Self::resolve_with(exe_dir, current_dir)
    }

    /// 주입된 실행 경로 및 작업 디렉터리를 기반으로 경로 및 실행 모드를 결정합니다.
    pub fn resolve_with(exe_dir: Option<PathBuf>, current_dir: Option<PathBuf>) -> Self {
        // 0. 작업 디렉터리 및 실행 디렉터리 후보 중 실제 프로젝트/앱 루트 탐색
        let base_bundle = match (exe_dir.as_deref(), current_dir.as_deref()) {
            // Cargo 빌드 디렉터리(target/debug 등) 내부 실행 시, 상위 디렉터리를 탐색하여 Cargo.toml이 있는 워크스페이스 루트를 확정
            (Some(exe), _) if is_cargo_target_dir(exe) => {
                if let Some(ws_root) = find_cargo_workspace_root(exe) {
                    ws_root
                } else if let Some(cwd) = current_dir.as_deref() {
                    cwd.to_path_buf()
                } else {
                    exe.to_path_buf()
                }
            }
            // 작업 디렉터리에 이미 사용자 설정이나 DB가 존재하는 경우 작업 디렉터리 우선
            (_, Some(cwd))
                if cwd.join("settings.user.json").exists()
                    || cwd.join("cache").join("record.db").exists()
                    || cwd.join(".portable").exists() =>
            {
                cwd.to_path_buf()
            }
            (Some(exe), _) => exe.to_path_buf(),
            (None, Some(cwd)) => cwd.to_path_buf(),
            (None, None) => PathBuf::from("."),
        };

        // 1. 환경변수 OVERMAX_DATA_DIR 강제 오버라이드
        if let Ok(custom_data) = std::env::var("OVERMAX_DATA_DIR") {
            let t = custom_data.trim();
            if !t.is_empty() {
                return Self::new_installed(base_bundle, PathBuf::from(t));
            }
        }

        // 2. 환경변수 OVERMAX_PORTABLE 강제 오버라이드
        if let Ok(portable_var) = std::env::var("OVERMAX_PORTABLE") {
            let t = portable_var.trim().to_ascii_lowercase();
            if t == "1" || t == "true" || t == "yes" {
                return Self::new_portable(base_bundle);
            }
        }

        // 3. 실행 파일 디렉터리 내 .portable 마커 파일 확인
        if base_bundle.join(".portable").is_file() {
            return Self::new_portable(base_bundle);
        }

        // 4. MSIX 패키지 런타임 환경 감지 (Windows Store)
        if is_running_in_msix_package() {
            let app_data = system_local_data_dir().unwrap_or_else(|| base_bundle.clone());
            return Self::new_installed(base_bundle, app_data);
        }

        // 5. 실행 디렉터리에 기존 포터블 데이터(settings.user.json / cache/record.db)가 이미 존재하고 쓰기 가능한 경우
        let has_existing_data = base_bundle.join("settings.user.json").exists()
            || base_bundle.join("cache").join("record.db").exists();
        if has_existing_data && is_dir_writable(&base_bundle) {
            return Self::new_portable(base_bundle);
        }

        // 6. 실행 디렉터리 쓰기 가능성 검사 (일반 zip 압축 해제본 기본값 = Portable)
        if is_dir_writable(&base_bundle) {
            return Self::new_portable(base_bundle);
        }

        // 7. 읽기 전용 디렉터리(Program Files 등)인 경우 -> Installed 모드 (%LOCALAPPDATA%\Overmax)
        let app_data = system_local_data_dir().unwrap_or_else(|| base_bundle.clone());
        Self::new_installed(base_bundle, app_data)
    }

    /// 현재 실행 모드
    pub fn mode(&self) -> RuntimeMode {
        self.mode
    }

    /// 포터블 모드 여부
    pub fn is_portable(&self) -> bool {
        self.mode == RuntimeMode::Portable
    }

    /// 읽기 전용 번들 에셋 디렉터리
    pub fn bundle_dir(&self) -> &Path {
        &self.bundle_dir
    }

    /// 가변 사용자 데이터 루트 디렉터리
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// 캐시 디렉터리 (`data_dir/cache`)
    pub fn cache_dir(&self) -> PathBuf {
        self.data_dir.join("cache")
    }

    /// 기본 설정 파일 경로 (`bundle_dir/settings.json`)
    pub fn settings_json(&self) -> PathBuf {
        self.bundle_dir.join("settings.json")
    }

    /// 사용자 변경 설정 파일 경로 (`data_dir/settings.user.json`)
    pub fn settings_user_json(&self) -> PathBuf {
        self.data_dir
            .join(DataCompatibility::current().settings_user_json)
    }

    /// 사용자 플레이 기록 SQLite DB 경로 (`data_dir/cache/record.db`)
    pub fn record_db(&self) -> PathBuf {
        self.data_dir.join(DataCompatibility::current().record_db)
    }

    /// V-Archive 곡 메타 DB 경로 (`data_dir/cache/songs.json`)
    pub fn songs_json(&self) -> PathBuf {
        self.data_dir.join(DataCompatibility::current().songs_json)
    }

    /// V-Archive DLC 메타 경로 (`data_dir/cache/dlcs.json`)
    pub fn dlcs_json(&self) -> PathBuf {
        self.data_dir.join(DataCompatibility::current().dlcs_json)
    }

    /// 자켓 이미지 인덱스 DB 경로 (`data_dir/cache/image_index.db`)
    pub fn image_index_db(&self) -> PathBuf {
        self.data_dir
            .join(DataCompatibility::current().image_index_db)
    }

    /// 패턴 시트 메타 캐시 경로 (`data_dir/cache/pattern_meta.json`)
    pub fn pattern_meta_json(&self) -> PathBuf {
        self.cache_dir().join("pattern_meta.json")
    }

    /// 로컬 IPC 엔드포인트 파일 경로 (`data_dir/cache/ipc_endpoint.json`)
    pub fn ipc_endpoint_json(&self) -> PathBuf {
        self.cache_dir().join("ipc_endpoint.json")
    }

    /// 디텍션 텔레메트리 로그 경로 (`data_dir/cache/telemetry.log`)
    pub fn telemetry_log(&self) -> PathBuf {
        self.cache_dir().join("telemetry.log")
    }

    /// 이전 디텍션 텔레메트리 로그 경로 (`data_dir/cache/telemetry.prev.log`)
    pub fn telemetry_prev_log(&self) -> PathBuf {
        self.cache_dir().join("telemetry.prev.log")
    }

    /// 레거시 V-Archive JSON 캐시 디렉터리 경로 (`data_dir/cache/varchive`)
    pub fn varchive_cache_dir(&self) -> PathBuf {
        self.cache_dir().join("varchive")
    }

    /// `SettingsPaths` 구조체 변환
    pub fn settings_paths(&self) -> SettingsPaths {
        SettingsPaths {
            settings_json: self.settings_json(),
            settings_user_json: self.settings_user_json(),
        }
    }

    /// 필수 데이터/캐시 디렉터리를 생성하고, Installed 모드 시 번들된 초기 캐시 에셋을 시딩/마이그레이션합니다.
    pub fn ensure_dirs_and_seed(&self) -> std::io::Result<()> {
        fs::create_dir_all(self.data_dir())?;
        fs::create_dir_all(self.cache_dir())?;

        if self.mode == RuntimeMode::Installed {
            // 번들된 캐시 파일이 존재하고, data_dir에 아직 없다면 복사 (초기 Cold Start 단축)
            self.seed_file_if_missing(
                &self.bundle_dir.join("cache").join("songs.json"),
                &self.songs_json(),
            )?;
            self.seed_file_if_missing(
                &self.bundle_dir.join("cache").join("dlcs.json"),
                &self.dlcs_json(),
            )?;
            self.seed_file_if_missing(
                &self.bundle_dir.join("cache").join("image_index.db"),
                &self.image_index_db(),
            )?;
            self.seed_file_if_missing(
                &self.bundle_dir.join("cache").join("pattern_meta.json"),
                &self.pattern_meta_json(),
            )?;

            // 기존 포터블 설치 위치에서 전환된 경우 사용자 설정 및 기록 DB 마이그레이션 복사
            self.seed_file_if_missing(
                &self.bundle_dir.join("settings.user.json"),
                &self.settings_user_json(),
            )?;
            self.seed_file_if_missing(
                &self.bundle_dir.join("cache").join("record.db"),
                &self.record_db(),
            )?;
        }

        Ok(())
    }

    fn seed_file_if_missing(&self, src: &Path, dst: &Path) -> std::io::Result<()> {
        if !dst.exists() && src.is_file() {
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            let _ = fs::copy(src, dst)?;
        }
        Ok(())
    }
}

/// OS별 표준 로컬 앱 데이터 디렉터리 경로 반환 (%LOCALAPPDATA%\Overmax 또는 XDG_DATA_HOME/overmax)
pub fn system_local_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        if let Some(val) = std::env::var_os("LOCALAPPDATA") {
            if !val.is_empty() {
                return Some(PathBuf::from(val).join("Overmax"));
            }
        }
        if let Some(val) = std::env::var_os("USERPROFILE") {
            if !val.is_empty() {
                return Some(
                    PathBuf::from(val)
                        .join("AppData")
                        .join("Local")
                        .join("Overmax"),
                );
            }
        }
        None
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(val) = std::env::var_os("XDG_DATA_HOME") {
            if !val.is_empty() {
                return Some(PathBuf::from(val).join("overmax"));
            }
        }
        if let Some(val) = std::env::var_os("HOME") {
            if !val.is_empty() {
                return Some(
                    PathBuf::from(val)
                        .join(".local")
                        .join("share")
                        .join("overmax"),
                );
            }
        }
        None
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

/// MSIX / AppX 패키지 런타임 내부에서 실행 중인지 감지
pub fn is_running_in_msix_package() -> bool {
    #[cfg(target_os = "windows")]
    {
        static CACHED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *CACHED.get_or_init(|| {
            use std::ffi::c_void;
            type GetCurrentPackageFullNameFn = unsafe extern "system" fn(
                package_full_name_length: *mut u32,
                package_full_name: *mut u16,
            ) -> i32;

            const APPMODEL_ERROR_NO_PACKAGE: i32 = 15700;

            unsafe {
                let kernel32 = windows_sys::Win32::System::LibraryLoader::GetModuleHandleA(
                    c"kernel32.dll".as_ptr().cast(),
                );
                if kernel32.is_null() {
                    return false;
                }
                let proc = windows_sys::Win32::System::LibraryLoader::GetProcAddress(
                    kernel32,
                    c"GetCurrentPackageFullName".as_ptr().cast(),
                );
                let Some(proc_addr) = proc else {
                    return false;
                };
                let get_package_full_name: GetCurrentPackageFullNameFn =
                    std::mem::transmute::<*const c_void, GetCurrentPackageFullNameFn>(
                        proc_addr as *const _,
                    );

                let mut length: u32 = 0;
                let result = get_package_full_name(&mut length, std::ptr::null_mut());
                result != APPMODEL_ERROR_NO_PACKAGE
            }
        })
    }

    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// 주어진 디렉터리가 cargo build artifact 디렉터리(target/debug 등)인지 검사
pub fn is_cargo_target_dir(dir: &Path) -> bool {
    let dir_str = dir.to_string_lossy();
    dir_str.ends_with("target\\debug")
        || dir_str.ends_with("target/debug")
        || dir_str.ends_with("target\\release")
        || dir_str.ends_with("target/release")
        || dir_str.contains("target\\debug\\")
        || dir_str.contains("target/debug/")
        || dir_str.contains("target\\release\\")
        || dir_str.contains("target/release/")
}

/// 주어진 빌드 디렉터리의 상위 디렉터리를 거슬러 올라가며 `Cargo.toml`이 존재하는 워크스페이스 루트 탐색
pub fn find_cargo_workspace_root(dir: &Path) -> Option<PathBuf> {
    let mut curr = dir.to_path_buf();
    while let Some(parent) = curr.parent() {
        if parent.join("Cargo.toml").exists() {
            return Some(parent.to_path_buf());
        }
        curr = parent.to_path_buf();
    }
    None
}

/// 디렉터리가 쓰기 가능한지 임시 파일 생성으로 검사
pub fn is_dir_writable(dir: &Path) -> bool {
    if !dir.exists() {
        return false;
    }
    let test_file = dir.join(format!(
        ".overmax_write_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&test_file)
    {
        Ok(f) => {
            drop(f);
            let _ = fs::remove_file(&test_file);
            true
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn portable_paths_match_root() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().to_path_buf();
        let paths = AppPaths::new_portable(root.clone());

        assert_eq!(paths.mode(), RuntimeMode::Portable);
        assert!(paths.is_portable());
        assert_eq!(paths.bundle_dir(), root.as_path());
        assert_eq!(paths.data_dir(), root.as_path());
        assert_eq!(paths.cache_dir(), root.join("cache"));
        assert_eq!(paths.settings_json(), root.join("settings.json"));
        assert_eq!(paths.settings_user_json(), root.join("settings.user.json"));
        assert_eq!(paths.record_db(), root.join("cache").join("record.db"));
        assert_eq!(paths.songs_json(), root.join("cache").join("songs.json"));
        assert_eq!(paths.dlcs_json(), root.join("cache").join("dlcs.json"));
        assert_eq!(
            paths.image_index_db(),
            root.join("cache").join("image_index.db")
        );
        assert_eq!(
            paths.ipc_endpoint_json(),
            root.join("cache").join("ipc_endpoint.json")
        );
    }

    #[test]
    fn installed_paths_separate_bundle_and_data() {
        let temp_bundle = TempDir::new().unwrap();
        let temp_data = TempDir::new().unwrap();
        let bundle_dir = temp_bundle.path().to_path_buf();
        let data_dir = temp_data.path().to_path_buf();

        let paths = AppPaths::new_installed(bundle_dir.clone(), data_dir.clone());

        assert_eq!(paths.mode(), RuntimeMode::Installed);
        assert!(!paths.is_portable());
        assert_eq!(paths.bundle_dir(), bundle_dir.as_path());
        assert_eq!(paths.data_dir(), data_dir.as_path());
        assert_eq!(paths.settings_json(), bundle_dir.join("settings.json"));
        assert_eq!(
            paths.settings_user_json(),
            data_dir.join("settings.user.json")
        );
        assert_eq!(paths.record_db(), data_dir.join("cache").join("record.db"));
        assert_eq!(
            paths.songs_json(),
            data_dir.join("cache").join("songs.json")
        );
    }

    #[test]
    fn ensure_dirs_and_seed_copies_bundle_caches() {
        let temp_bundle = TempDir::new().unwrap();
        let temp_data = TempDir::new().unwrap();
        let bundle_cache = temp_bundle.path().join("cache");
        fs::create_dir_all(&bundle_cache).unwrap();

        fs::write(bundle_cache.join("songs.json"), b"{\"songs\": []}").unwrap();
        fs::write(bundle_cache.join("image_index.db"), b"dummy-sqlite-image").unwrap();
        fs::write(
            temp_bundle.path().join("settings.user.json"),
            b"{\"seeded\": true}",
        )
        .unwrap();

        let paths = AppPaths::new_installed(temp_bundle.path(), temp_data.path());
        paths.ensure_dirs_and_seed().unwrap();

        assert!(paths.cache_dir().exists());
        assert_eq!(
            fs::read_to_string(paths.songs_json()).unwrap(),
            "{\"songs\": []}"
        );
        assert_eq!(
            fs::read(paths.image_index_db()).unwrap(),
            b"dummy-sqlite-image"
        );
        assert_eq!(
            fs::read_to_string(paths.settings_user_json()).unwrap(),
            "{\"seeded\": true}"
        );
    }

    #[test]
    fn marker_file_forces_portable_mode() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().to_path_buf();
        fs::write(root.join(".portable"), b"").unwrap();

        let paths = AppPaths::resolve_with(Some(root.clone()), None);
        assert_eq!(paths.mode(), RuntimeMode::Portable);
        assert_eq!(paths.data_dir(), root.as_path());
    }

    #[test]
    fn settings_paths_matches_app_paths() {
        let temp_bundle = TempDir::new().unwrap();
        let temp_data = TempDir::new().unwrap();
        let paths = AppPaths::new_installed(temp_bundle.path(), temp_data.path());
        let sp = paths.settings_paths();

        assert_eq!(sp.settings_json, temp_bundle.path().join("settings.json"));
        assert_eq!(
            sp.settings_user_json,
            temp_data.path().join("settings.user.json")
        );
    }

    #[test]
    fn ensure_dirs_and_seed_copies_record_db_and_meta() {
        let temp_bundle = TempDir::new().unwrap();
        let temp_data = TempDir::new().unwrap();
        let bundle_cache = temp_bundle.path().join("cache");
        fs::create_dir_all(&bundle_cache).unwrap();

        fs::write(bundle_cache.join("record.db"), b"dummy-sqlite-record").unwrap();
        fs::write(
            bundle_cache.join("pattern_meta.json"),
            b"{\"patterns\": []}",
        )
        .unwrap();
        fs::write(bundle_cache.join("dlcs.json"), b"{\"dlcs\": []}").unwrap();

        let paths = AppPaths::new_installed(temp_bundle.path(), temp_data.path());
        paths.ensure_dirs_and_seed().unwrap();

        assert_eq!(fs::read(paths.record_db()).unwrap(), b"dummy-sqlite-record");
        assert_eq!(
            fs::read_to_string(paths.pattern_meta_json()).unwrap(),
            "{\"patterns\": []}"
        );
        assert_eq!(
            fs::read_to_string(paths.dlcs_json()).unwrap(),
            "{\"dlcs\": []}"
        );
    }

    #[test]
    fn cargo_target_dir_resolves_to_workspace_cwd() {
        let temp_workspace = TempDir::new().unwrap();
        let ws_root = temp_workspace.path().to_path_buf();
        let target_debug = ws_root.join("target").join("debug");
        fs::create_dir_all(&target_debug).unwrap();
        fs::write(ws_root.join("Cargo.toml"), b"[workspace]").unwrap();
        fs::write(
            ws_root.join("settings.user.json"),
            b"{\"debug\": {\"enabled\": true}}",
        )
        .unwrap();

        let paths = AppPaths::resolve_with(Some(target_debug.clone()), Some(ws_root.clone()));

        assert_eq!(paths.mode(), RuntimeMode::Portable);
        assert_eq!(paths.data_dir(), ws_root.as_path());
        assert_eq!(
            paths.settings_user_json(),
            ws_root.join("settings.user.json")
        );

        // cwd가 target_debug이거나 None인 경우에도 상위 탐색을 통해 workspace root를 찾아냄
        let paths_cwd_in_target =
            AppPaths::resolve_with(Some(target_debug.clone()), Some(target_debug));
        assert_eq!(paths_cwd_in_target.mode(), RuntimeMode::Portable);
        assert_eq!(paths_cwd_in_target.data_dir(), ws_root.as_path());
    }
}
