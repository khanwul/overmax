# Overmax v0.4.1 릴리즈 노트

> v0.4.0 이후 변경 사항 (v0.4.1)

---

## 🎮 새로워진 점 및 개선 사항

### 📡 1. 실시간 외부 방송 연동 및 도구 확장 지원 (로컬 IPC 프로토콜 도입)
* **[체감 변화]**: OBS 방송 화면 오버레이, 외부 선곡 위젯, 서드파티 봇 프로그램에서 현재 플레이 중인 곡과 추천 목록 정보를 실시간으로 손쉽게 연동할 수 있습니다.
* **[개선 세부 요약]**:
  * **실시간 이벤트 스트리밍 (SSE)**: 게임 중 화면이 바뀌거나 선곡 화면에서 다른 곡으로 이동할 때, 결과창에서 점수가 확정되는 순간의 모든 플레이 정보를 0.001초 단위의 실시간 스트림(`GET /events`)으로 외부에 브로드캐스트합니다.
  * **외부 원격 제어 인터페이스 (JSON-RPC 2.0)**: 외부 도구에서 Overmax에 접속하여 현재 곡 정보 조회(`game.current_song`), 맞춤 추천 곡 목록 요청(`recommend.get_candidates`), 오버레이 표시 여부 전환(`overlay.set_visibility`) 등을 호출할 수 있는 표준 원격 호출 규격을 지원합니다.
  * **충돌 없는 안전한 단일 포트 통신**: 외부 인터넷 연결 없이 플레이어 PC 내부(Localhost `127.0.0.1`)에서만 동작하며, 포트가 중복될 경우 안전한 대역(30100~30199)을 자동으로 탐색하여 다른 프로그램과의 충돌을 방지합니다.
* 💡 **이용 팁**: `설정(⚙) > 고급 > 로컬 IPC 통신` 섹션에서 원클릭으로 켜고 끌 수 있으며, 연결된 클라이언트 수와 실제 할당된 포트 번호를 실시간 뱃지로 확인할 수 있습니다. 기본 설정은 꺼짐(OFF) 상태로 안전하게 유지됩니다.

### 📦 2. Windows 스토어(MSIX) 및 설치형 환경 지원
* **[체감 변화]**: 포터블 압축 파일(Zip) 외에도 향후 Microsoft Store 및 Windows 공식 앱 패키지(MSIX) 환경에서 시작 메뉴 검색과 원클릭 설치로 편리하게 게임에 진입할 수 있습니다.
* **[개선 세부 요약]**:
  * **스토어 런타임 환경 자동 감지**: 앱이 Windows Store/MSIX 패키지 환경에서 실행 중인지 자동으로 감지하여, 스토어 전용 보안 저장소(`%LOCALAPPDATA%\Overmax`)로 플레이 기록과 설정을 안전하게 격리·보존합니다.
  * **초기 캐시 자동 마이그레이션**: 앱을 새로 설치하더라도 번들된 기본 곡 정보와 커버 이미지 캐시를 자동으로 동기화하여 첫 실행 시의 지연 시간(Cold Start) 없이 즉시 오버레이가 작동합니다.
  * **포터블/설치형 완벽 호환**: 기존 포터블 압축본 사용자는 이전과 동일하게 실행 파일이 위치한 폴더에 모든 설정과 기록이 보존되는 포터블 모드로 그대로 유지됩니다.

### 🛡️ 3. 스토어 환경 자가 업데이터 스마트 분기
* **[체감 변화]**: 스토어 패키지로 실행할 때는 외부 파일 교체로 인한 충돌이나 보안 경고 없이 안전하게 Microsoft Store의 정식 업데이트를 안내받을 수 있습니다.
* **[개선 세부 요약]**:
  * 스토어 모드에서는 GitHub Releases 바이너리 교체 방식의 인앱 업데이터가 자동으로 비활성화되며, `설정(⚙) > 일반` 탭에 "Microsoft Store 패키지(스토어를 통해 자동으로 최신 버전이 유지됩니다)"라는 안내 뱃지가 표시되어 혼선을 방지합니다.

---

## 🛠️ 엔지니어링 & 내부 아키텍처 변경점

### 🌐 1. std-only 무의존성 로컬 IPC 서버 및 SSE/JSON-RPC 2.0 통합 아키텍처
* **[Zero-Dependency 경량 Loopback HTTP 서버]**:
  * 외부 HTTP 웹 프레임워크(actix, axum 등) 추가 없이 순수 Rust 표준 라이브러리(`std::net::TcpListener`, `std::net::TcpStream`)만으로 구현된 경량 non-blocking IPC 서버(`overmax_app::system::ipc_server`) 구축 (추가 바이너리 크기 증가 0KB).
  * `127.0.0.1` 단일 인터페이스 강제 바인딩으로 외부 네트워크 접근을 원천 차단. 기본 포트 30110 기준, 이미 사용 중일 경우 30100~30199 대역 내 순차 탐색 후 `cache/ipc_endpoint.json` 파일에 확정 포트를 원자적으로(tempfile + atomic rename) 기록.
* **[Dual Transport: Server-Sent Events (SSE) & JSON-RPC 2.0]**:
  * **SSE 스트리밍 (`GET /events`)**: `overmax-ipc/1` 규격의 단방향 이벤트 스트림. 멀티스레드 크로스 채널을 통해 `SceneChangeEvent`, `SongChangeEvent`, `VerifiedPlayEvent`를 JSON 직렬화하여 연결된 모든 클라이언트에 브로드캐스트.
  * **JSON-RPC 2.0 (`POST /rpc`)**:
    * `system.version`, `system.status`: 런타임 상태 및 버전 질의
    * `game.current_song`: 현재 인식된 곡명, 난이도, 버튼 모드 조회
    * `recommend.get_candidates`: 현재 실력 기준 스마트 추천 엔트리 목록 질의
    * `overlay.set_visibility`: 오버레이 창 표시/숨김 원격 제어

### 📂 2. 데이터 경로 추상화 레이어 (`AppPaths`) & 듀얼 모드 분기
* **[바이너리-상대 경로 vs 시스템 로컬 앱데이터 격리]**:
  * `overmax_data::AppPaths` 구조체를 신설하여 `bundle_dir`(읽기 전용 번들 에셋)과 `data_dir`(가변 사용자 데이터)을 추상화.
  * Win32 `GetCurrentPackageFullName` C-API 동적 바인딩을 통해 런타임 오버헤드 0의 정적 캐싱(`OnceLock<bool>`)으로 MSIX 패키지 실행 여부를 무비용 감지.
  * **모드 확정 우선순위 엔진**:
    1. 환경변수 `OVERMAX_DATA_DIR` 강제 오버라이드
    2. 환경변수 `OVERMAX_PORTABLE=1` 또는 `.portable` 마커 파일 확인
    3. `is_running_in_msix_package()` 감지 ➔ Installed 모드 (`%LOCALAPPDATA%\Overmax\`)
    4. 실행 디렉터리 쓰기 가능성(`is_dir_writable`) 검사 ➔ 쓰기 가능 시 Portable 모드 기본 유지
    5. 쓰기 불가(Program Files 등) 시 ➔ Installed 모드 자동 폴백
  * **Cold Start 단축 시딩 (`ensure_dirs_and_seed`)**: Installed 모드 최초 기동 시 번들된 `songs.json`, `dlcs.json`, `image_index.db`, `pattern_meta.json`을 사용자 로컬 캐시 디렉터리로 원자적 복사.

### 📦 3. Desktop Bridge(Centennial) 패키징 파이프라인 & 버전 매핑 정책
* **[Desktop Bridge AppxManifest 및 Visual Assets 규격화]**:
  * `AppxManifest.xml.template` 작성: `runFullTrust` 권한, Win32 `Windows.FullTrustApplication` 엔트리포인트 및 타일 에셋 매핑.
  * 1254×1254 고해상도 로고로부터 Bicubic 보간을 적용한 MSIX 필수 비주얼 에셋 6종 일괄 생성 (`Square44x44`, `Square150x150`, `Wide310x150`, `Square310x310`, `StoreLogo`, `SplashScreen`).
* **[원스톱 빌드/서명/사이드로딩 자동화 (`package-msix.ps1`)]**:
  * Windows 10/11 SDK 도구(`MakeAppx.exe`, `SignTool.exe`) 자동 탐색.
  * 개발용 자체 서명 인증서(`OvermaxDev.pfx`, `OvermaxDev.cer`) 자동 생성 및 `LocalMachine\TrustedPeople` 신뢰 체인 안내 로직 탑재.
  * XML 특수문자(`&` 등) 자동 이스케이프 방어.
* **[SemVer ➔ MSIX 4단위 버전 규격화 (Major.Minor.Build.0)]**:
  * Microsoft Store 패키지 승인 정책상 4번째 자리(수정 번호, Revision)는 스토어 내부 관리용으로 예약되어 있어 반드시 `0`이어야 하는 제약 반영:
    * **프리뷰/정식 공통**: `X.Y.Z(-preview<N>)` ➔ `X.Y.Z.0` (예: `0.4.1-preview1` ➔ `0.4.1.0`)
    * `scripts/package-msix.ps1`에서 `Major.Minor.Build.0`으로 자동 정규화.
* **[스토어 정책 및 CI 연동]**:
  * Microsoft Partner Center 심사용 영/한 앱 설명, 서드파티 면책 조항(Disclaimer), `runFullTrust` 소명서, 오픈소스 개인정보처리방침([`docs/store/PRIVACY.md`](../store/PRIVACY.md)) 완비.
  * GitHub Actions Windows 워크플로우에 `dist/*.msix` 아티팩트 빌드 단계 연동.