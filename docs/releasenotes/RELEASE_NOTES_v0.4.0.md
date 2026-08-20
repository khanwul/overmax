# Overmax v0.4.0 릴리즈 노트

> v0.3.3 이후 변경 사항 (v0.4.0)  
> 기준 커밋: `6256097f48bb5292e3e50b293f6c46db0136cdeb`

---

## 🎮 새로워진 점 및 개선 사항

### 🧹 1. 미플레이 곡 점수 잔류 방지 및 과거 오기록 자동 정화
* **[체감 변화]**: 이전에 플레이했던 곡의 점수가 새로 선택한 미플레이 곡 화면에 잘못 남아있던 현상을 완전히 차단했습니다.
* **[개선 세부 요약]**: 
  * 화면 인식 캐시 구조를 개편하여 곡 목록을 넘길 때 이전 곡의 점수나 콤보 데이터가 다른 곡으로 넘어가지 않도록 즉시 분리·초기화합니다.
  * 과거에 잘못 인식되어 로컬 기록에 남아있던 유령 기록(Ghost Records)이 있더라도, 곡 목록 화면에서 미플레이(0.00%) 상태를 확인하는 즉시 과거 오기록을 로컬 데이터에서 자동으로 삭제하여 추천 목록과 오버레이가 항상 깨끗하고 정확하게 유지됩니다.

### 🔄 2. 곡 목록 탐색 중 로컬 기록 자동 동기화 (앱 내부)
* **[체감 변화]**: 게임 내 곡 목록을 둘러보는 것만으로도 화면에 표시된 내 최신 기록이 Overmax 내부에 즉시 반영되어 맞춤 추천 정확도가 높아집니다.
* **[개선 세부 요약]**: 곡 목록 화면에서 커서가 위치한 곡의 모드/난이도/점수가 안정적으로 인식되면, Overmax의 내부 로컬 데이터베이스에 자동으로 기록되어 추천 곡 목록과 난이도 탭 정보를 최신 상태로 갱신합니다.
* 💡 **안내**: 이는 **Overmax 앱 내부의 로컬 기록 동기화**이며, V-Archive 웹사이트로 기록이 자동 업로드되는 것은 아닙니다. V-Archive 기록 전송은 이전과 동일하게 플레이어께서 직접 전송 버튼을 눌러 등록하실 수 있습니다.

### ⚡ 3. 이벤트 기반 처리 도입으로 인게임 버벅임 완화
* **[체감 변화]**: 화면을 분석하고 기록을 저장하는 과정에서 발생할 수 있었던 미세한 끊김(Stuttering)을 줄였습니다.
* **[개선 세부 요약]**: 매 프레임마다 화면을 검사하여 저장하던 기존 방식을 개선하여, 결과창이나 선곡창에서 기록이 완전히 확정되는 순간에만 단 1회 저장하도록 이벤트 방식으로 전환함으로써 불필요한 시스템 부하를 소거했습니다.

---

## 🛠️ 엔지니어링 & 내부 아키텍처 변경점

### 🎯 1. 이벤트 주도형 `VerifiedPlayEvent` 도메인 아키텍처 도입
* **[도메인 이벤트 분리 및 Zero-Allocation 전송]**:
  * 메인 UI 렌더 루프(`native_app`)가 매 프레임 `output.state.context`를 폴링하여 DB 쓰기 여부를 평가하던 절차적 방식을 전면 제거했습니다.
  * `PlayStateDetector`가 5프레임 연속 안정화(Hysteresis) 시점에 단일 `VerifiedPlayEvent` (Copy/Zero-Allocation 값 객체)를 단 1회(또는 기록 개선 시) 방출하고, `RecordManager::handle_verified_play`가 이를 직접 수신하여 SQLite DB 및 캐시를 동기화하도록 이벤트 주도 계층 분리를 완성했습니다.

### 🧱 2. Data-First `RoiCache` 및 `ResultModeDiffLatch` 캡슐화
* **[관측 캐시와 보정 래치의 도메인 책임 격리]**:
  * 산발적인 수동 캐시 조작으로 인한 점수 잔류 버그를 원천 차단하기 위해 `RoiCache<Key, Checksum, Value>` 제네릭 구조체를 도입하고, 관측 단위와 체크섬을 단일 캐시로 캡슐화했습니다. 키(`RecordKey`) 변경 시 이전 캐시를 즉시 무효화합니다.
  * 결과창 애니메이션 깜빡임 보정용 래치(`ResultModeDiffLatch`)와 일반 관측 캐시의 수명주기를 분리하고, 플레이 상태를 `PatternRecord::Unplayed`와 `PatternRecord::Played`의 대수적 데이터 타입(ADT)으로 명시적으로 모델링했습니다.

### 🚀 3. 템플릿 매칭 Zero-Allocation 및 u32 비트마스크/Popcount 최적화
* **[스택 기반 비트 패킹 및 CPU Popcount 연산 전환]**:
  * 글자당 20여 회 발생하던 동적 `Vec<u8>` 힙 할당을 스택 기반 `[u32; 32]` 비트마스크 패킹 및 CPU `count_ones()`(popcnt) 연산으로 전환하여 매칭 연산 속도를 10~20배 고속화하고 힙 할당을 0으로 줄였습니다.
  * `detect_score`의 `char → String → parse → u32` 중복 왕복을 `score * 10 + digit` 정수 직접 누적으로 직결하고, `detect_rate` 내 미사용 `String` 필드를 제거하여 Zero String Allocation 파이프라인을 구축했습니다.

### 🌐 4. 크레이트 경계 책임 정돈 및 네트워크/ROI 설정 단일화
* **[HTTP 네트워크 I/O 및 ROI 설정 위치 일원화]**:
  * UI 계층(`overmax_app`)에 산재해 있던 V-Archive HTTP 통신(`cache_downloader`, `varchive_api`, `recommend_provider_fetch`)을 `overmax_data` 계층으로 완전히 통합했습니다.
  * ROI 설정 구조체(`GlobalRoiConfig`, `SceneRoiConfig`)를 `overmax_data`에서 `overmax_engine::detector::roi_config`로 이관하여 도메인 경계 누수를 해소했습니다.

### 🛡️ 5. 데이터 무결성 및 동시성 가드 강화
* **[SQLite WAL 모드 및 Atomic Settings Debounce Writer]**:
  * `RecordDB`에 SQLite WAL(Write-Ahead Logging) 모드, Busy Timeout(3초), 지수 백오프 재시도 가드를 적용하여 멀티스레드 DB 쓰기 안정성을 확보했습니다.
  * `settings.user.json` 쓰기 시 100ms 디바운스 및 임시 파일 교체(`settings.user.json.tmp`)를 통한 원자적(Atomic) 파일 저장을 적용했습니다.

### 📦 6. 워크스페이스 크레이트 버전 단일화 (`version.workspace = true`)
* 워크스페이스 내 5개 크레이트(`overmax_core`, `overmax_cv`, `overmax_data`, `overmax_engine`, `overmax_app`)의 패키지 버전을 루트 `Cargo.toml`의 `workspace.package.version`으로 통일했습니다.
