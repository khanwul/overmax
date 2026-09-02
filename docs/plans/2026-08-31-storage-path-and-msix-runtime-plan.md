# 데이터 저장소 경로 추상화 및 MSIX 런타임 분기 설계 계획 (v0.4.1)

**작성일**: 2026-08-31  
**작업 브랜치**: `feat/storage-path-and-msix-runtime`  
**관련 마일스톤**: v0.4.1 (TASK 2.1 & 2.2)  
**상태**: 계획 수립 (Planning)  

---

## 1. 배경 및 목적 (Motivation & Scope)

### 1.1 배경
Overmax는 현재 실행 파일 위치를 기준으로 한 상대 경로(`cache/record.db`, `settings.user.json`, `cache/songs.json` 등)를 기반으로 작동하고 있습니다.  
그러나 **MSIX 패키징 및 Microsoft Store 배포 환경**에서는 다음과 같은 제약과 요구사항이 발생합니다:

1. **설치 디렉터리 Read-Only 제약**:
   * MSIX로 설치된 프로그램의 설치 경로(`C:\Program Files\WindowsApps\...`)는 쓰기가 원천 차단됩니다.
   * Desktop Bridge (`runFullTrust`)의 파일 시스템 가상화(VFS) 리다이렉션에만 의존할 경우 파일 잠금, 백업, 마이그레이션 및 외부 도구 연동 시 경로 혼선이 발생할 수 있습니다.
2. **자가 업데이터 정책 충돌**:
   * Microsoft Store 정책상 인앱에서 자체적으로 바이너리를 교체/패치하는 자체 업데이터(`self_update` 기반 GitHub Releases 다운로더)는 금지되며, 스토어 자체의 델타 업데이트 엔진이 관리해야 합니다.
3. **기존 사용자 및 Portable 모드 100% 호환성 유지**:
   * zip 압축 해제 후 단일 폴더에서 사용하는 기존 플레이어(Portable 환경)와 개발자 환경의 워크플로우를 절대 깨뜨리지 않아야 합니다.

### 1.2 목적
* **데이터 저장소 경로 추상화 (`AppPaths`)**: Portable 모드(실행 디렉터리 기준)와 Installed 모드(`%LOCALAPPDATA%\Overmax\`)를 투명하게 지원하는 단일 경로 공급자 구축.
* **런타임 환경 감지 (`RuntimeEnvironment`)**: Win32 `GetCurrentPackageFullName` API를 통해 MSIX 패키지 실행 여부를 0-Cost로 정확히 판별.
* **자가 업데이터 분기 처리**: Store/MSIX 패키지 환경에서는 자체 업데이터를 안전하게 비활성화하고, 스토어 안내 UI를 제공.

---

## 2. 파일 및 데이터 자산 전수 맵 (Storage Asset Map)

| 자산명 | 접근 권한 | 설명 | Portable 위치 (기존) | Installed / MSIX 위치 (신규) |
| :--- | :---: | :--- | :--- | :--- |
| `settings.json` | Read-Only | 번들된 기본 설정 템플릿 | `<root>/settings.json` | `<root>/settings.json` (앱 패키지 내부) |
| `settings.user.json` | Read-Write | 사용자 커스텀 설정 델타 | `<root>/settings.user.json` | `%LOCALAPPDATA%\Overmax\settings.user.json` |
| `cache/record.db` | Read-Write | 플레이 기록 로컬 SQLite DB | `<root>/cache/record.db` | `%LOCALAPPDATA%\Overmax\cache\record.db` |
| `cache/songs.json` | Read-Write | V-Archive 곡 메타 DB | `<root>/cache/songs.json` | `%LOCALAPPDATA%\Overmax\cache\songs.json` |
| `cache/image_index.db`| Read-Write | 곡 자켓 인식 매칭 SQLite DB | `<root>/cache/image_index.db` | `%LOCALAPPDATA%\Overmax\cache\image_index.db` |
| `cache/dlcs.json` | Read-Write | DLC 메타데이터 캐시 | `<root>/cache/dlcs.json` | `%LOCALAPPDATA%\Overmax\cache\dlcs.json` |
| `cache/pattern_meta.json`| Read-Write | 커뮤니티 서열표 메타 캐시 | `<root>/cache/pattern_meta.json` | `%LOCALAPPDATA%\Overmax\cache\pattern_meta.json` |
| `cache/ipc_endpoint.json`| Read-Write | 로컬 IPC 포트 확정 파일 | `<root>/cache/ipc_endpoint.json` | `%LOCALAPPDATA%\Overmax\cache\ipc_endpoint.json` |
| `update.ok` | Read-Write | 자가 업데이트 성공 마커 | `<root>/update.ok` | `%LOCALAPPDATA%\Overmax\update.ok` (Portable만 사용) |
| `logs/` | Read-Write | 텔레메트리/디버그 로그 | `<root>/logs/` | `%LOCALAPPDATA%\Overmax\logs\` |

> **💡 Linux 환경 대응**:  
> Linux의 경우 XDG 기본 디렉터리 규격에 따라 `Installed` 모드 시 `$XDG_DATA_HOME/overmax/` (기본값: `~/.local/share/overmax/`)를 사용합니다.

---

## 3. 핵심 아키텍처 및 세부 설계

```
┌────────────────────────────────────────────────────────────────────────┐
│                        AppPaths Resolution Flow                        │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
               ┌────────────────────┴────────────────────┐
               ▼                                         ▼
   [ Explicit CLI Flag? ]                    [ Auto-Detection Rules ]
   • --portable  ➔ Portable Mode             1. Is '.portable' marker present? ➔ Portable
   • --data-dir  ➔ Custom Data Dir           2. Does local 'settings.user.json' exist? ➔ Portable
                                             3. Is running in MSIX Package? ➔ Installed
                                             4. Default fallback ➔ Portable (dev/zip match)
                                    │
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                              AppPaths                                  │
│  ├─ root_dir: PathBuf        (바이너리/번들 에셋 디렉터리, Read-Only)  │
│  ├─ data_dir: PathBuf        (사용자 데이터 디렉터리, Read-Write)      │
│  ├─ cache_dir: PathBuf       (캐시 & SQLite DB 디렉터리, Read-Write)  │
│  ├─ settings_json()          ➔ <root_dir>/settings.json               │
│  ├─ settings_user_json()     ➔ <data_dir>/settings.user.json          │
│  ├─ record_db()              ➔ <cache_dir>/record.db                  │
│  ├─ songs_json()             ➔ <cache_dir>/songs.json                 │
│  ├─ image_index_db()         ➔ <cache_dir>/image_index.db             │
│  └─ ipc_endpoint_json()      ➔ <cache_dir>/ipc_endpoint.json          │
└────────────────────────────────────────────────────────────────────────┘
```

### 3.1 `AppPaths` 구조체 설계 (`overmax_data::config::paths`)

기존 `SettingsPaths`를 확장·일원화하여 애플리케이션의 모든 데이터 경로를 공급하는 단일 책임 구조체를 정의합니다.

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageMode {
    /// 단일 폴더 기반 포터블 모드 (바이너리 기준 상대 경로)
    Portable,
    /// OS 표준 AppData 디렉터리 기반 모드 (%LOCALAPPDATA%\Overmax)
    Installed,
}

#[derive(Clone, Debug)]
pub struct AppPaths {
    mode: StorageMode,
    root_dir: PathBuf,
    data_dir: PathBuf,
    cache_dir: PathBuf,
}

impl AppPaths {
    /// 런타임 환경 및 로컬 파일 상태를 분석하여 최적의 AppPaths를 생성합니다.
    pub fn resolve() -> Self;

    /// 명시적 모드로 AppPaths를 생성합니다 (테스트 및 CLI 플래그용).
    pub fn from_mode(mode: StorageMode, root_dir: PathBuf) -> Self;

    /// 데이터 디렉터리 및 캐시 디렉터리가 없으면 원자적으로 생성합니다.
    pub fn ensure_directories(&self) -> std::io::Result<()>;

    pub fn settings_json(&self) -> PathBuf { self.root_dir.join("settings.json") }
    pub fn settings_user_json(&self) -> PathBuf { self.data_dir.join("settings.user.json") }
    pub fn record_db(&self) -> PathBuf { self.cache_dir.join("record.db") }
    pub fn songs_json(&self) -> PathBuf { self.cache_dir.join("songs.json") }
    pub fn image_index_db(&self) -> PathBuf { self.cache_dir.join("image_index.db") }
    pub fn ipc_endpoint_json(&self) -> PathBuf { self.cache_dir.join("ipc_endpoint.json") }
    pub fn logs_dir(&self) -> PathBuf { self.data_dir.join("logs") }
}
```

### 3.2 MSIX 런타임 패키지 감지 (`overmax_app::system::runtime_env`)

Windows Win32 `GetCurrentPackageFullName` API를 사용하여 0-overhead로 패키지 환경을 판별합니다.

```rust
pub struct RuntimeEnvironment;

impl RuntimeEnvironment {
    /// 현재 프로세스가 MSIX / AppX 패키지 컨테이너 내부에서 실행 중인지 검사합니다.
    #[cfg(windows)]
    pub fn is_msix_packaged() -> bool {
        use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
        use windows_sys::Win32::Foundation::{ERROR_SUCCESS, ERROR_INSUFFICIENT_BUFFER, WIN32_ERROR};

        let mut length: u32 = 0;
        let res = unsafe { GetCurrentPackageFullName(&mut length, std::ptr::null_mut()) };
        // ERROR_INSUFFICIENT_BUFFER(122) 또는 ERROR_SUCCESS(0)이면 패키지 환경임.
        // APPMODEL_ERROR_NO_PACKAGE(15700)이면 일반 실행(Unpackaged)임.
        res == ERROR_SUCCESS || res == 122
    }

    #[cfg(not(windows))]
    pub fn is_msix_packaged() -> bool {
        false
    }
}
```

### 3.3 자체 업데이터(Self-Updater) 스토어 분기

1. **`AppUpdateConfig` 확장**:
   * `is_store_package: bool` 속성을 추가.
   * `is_store_package == true`인 경우:
     * 백그라운드 버전 체크 비활성화 (`check_on_startup = false`)
     * 업데이트 다운로드 및 프로세스 교체 루틴 스킵
     * `notify_previous_update` 스킵
2. **Settings UI (설정창) 적응**:
   * General 탭의 "업데이트" 영역에서 수동 업데이트 버튼 대신:
     * `ℹ️ Microsoft Store를 통해 최신 버전이 자동으로 유지됩니다.` 텍스트 안내 표시.

### 3.4 데이터 초기화 및 마이그레이션 전략 (Safe Seeding)

* `StorageMode::Installed`로 첫 기동 시:
  * `%LOCALAPPDATA%\Overmax\cache\` 폴더 자동 생성.
  * 번들된 `songs.json` 또는 `image_index.db`가 `root_dir/cache/`에 존재하고, `%LOCALAPPDATA%`에는 아직 없다면 최초 1회 안전하게 초기 캐시로 복사 (Seed Cache).
  * 기존 데이터가 이미 존재한다면 덮어쓰지 않고 기존 사용자 데이터를 그대로 보존.

---

## 4. 단계별 실행 로드맵 (Phased Implementation)

### Phase 1: `AppPaths` 및 `StorageMode` 코어 구현 (`overmax_data`)
- [ ] `overmax_data::config::paths` 모듈 생성
- [ ] `StorageMode` 및 `AppPaths` 구조체, 경로 해석 로직 구현
- [ ] `SettingsPaths`를 `AppPaths`와 호환되도록 정리
- [ ] `AppPaths` 단위 테스트 작성 (Portable 모드, Installed 모드, Seed 복사 검증)

### Phase 2: 전역 데이터 경로 참조부 마이그레이션
- [ ] `overmax_app::ui::native_app::run_native_app()` 진입점에서 `AppPaths::resolve()` 도입
- [ ] `SettingsPaths::in_dir` 호출부를 `AppPaths` 참조로 전환
- [ ] `RecordDB::open`, `VArchiveDB::load_from_path`, `ImageIndexDb::open` 호출부에 `AppPaths` 경로 전달
- [ ] `ipc_endpoint.json`, `cache_update.rs`, `varchive_upload.rs` 경로 연동

### Phase 3: MSIX 런타임 감지 및 업데이터 분기
- [ ] `overmax_app::system::runtime_env` 모듈 구현 (`is_msix_packaged()`)
- [ ] `AppUpdateConfig`에 `is_store_package` 연동 및 백그라운드 업데이터 바이패스
- [ ] `settings_ui.rs` 업데이트 섹션에 스토어 패키지 안내 UI 적용

### Phase 4: CLI 플래그 및 회귀 검증
- [ ] `--portable` / `--data-dir <PATH>` CLI 인자 처리 지원
- [ ] `cargo test --workspace` 전체 통과 검증
- [ ] `cargo clippy --all-targets` 및 `cargo fmt` 정리

---

## 5. 불변 조건 및 품질 검증 (Invariants & QA)

1. **기존 사용자 데이터 불변**: 기존 포터블 zip 사용자의 `settings.user.json` 및 `cache/record.db`가 초기화되거나 유실되지 않는다.
2. **Zero Overhead**: 경로 해석은 앱 시작 시 $O(1)$로 1회만 수행되며 렌더링/디텍션 틱 루프에 부하를 주지 않는다.
3. **Fail-Closed 안전성**: `%LOCALAPPDATA%` 접근 권한 오류 발생 시 명확한 에러 다이얼로그를 띄우고 안전하게 종료한다.
