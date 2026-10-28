# System Overview Architecture

이 문서는 Overmax의 전체 워크스페이스 구조, 크레이트 간 단방향 의존성, 런타임 스레드 모델 및 크로스 스레드 메시지 흐름을 설명한다.

---

## 1. 워크스페이스 구조 및 크레이트 역할

Overmax는 단일 Rust 워크스페이스로 구성되어 있으며, 6개의 내부 크레이트가 명확한 단방향 계층 구조를 갖는다. `overmax_runtime`은 감지 출력(`GameSessionState`, `VerifiedPlayEvent`)을 `overmax_core` 타입으로만 받으므로 `overmax_engine`에 의존하지 않는다. `overmax_app`을 제외한 크레이트는 egui/eframe에 의존하지 않는다 (계약: [module-contracts.md](../guides/module-contracts.md)).

```
               [ overmax_app ]
                 │         │
                 ▼         ▼
     [ overmax_runtime ]  [ overmax_engine ]
                 │         │
                 ▼         ▼
               [ overmax_data ]
                     │
                     ▼
               [ overmax_cv ]
                     │
                     ▼
               [ overmax_core ]
```

### 크레이트별 책임과 의존성

| 크레이트 | 경로 | 책임 | 주요 의존 크레이트 |
|---|---|---|---|
| `overmax_core` | `rust/overmax_core` | 핵심 도메인 모델(`GameState`, `PlayContext`, `Song`, `RecordKey`, `RecordValue`, `Mode`, `Difficulty`, `SceneType`) 및 공용 타입 정의. 무거운 외부 의존성을 배제한 순수 데이터 계약 계층. | 없음 (최하위) |
| `overmax_cv` | `rust/overmax_cv` | OpenCV 없이 구현된 순수 Rust CV 알고리즘 백엔드 (pHash, dHash, aHash, HOG, DCT, Grayscale/Resize 변환, 4x4 RGBA 히스토그램 Centroid). | `overmax_core` |
| `overmax_data` | `rust/overmax_data` | 설정 파일 관리(`Settings`), SQLite 로컬 저장소(`RecordDB`), V-Archive API 클라이언트 및 동기화(`sync`), 자켓 피처 매칭(`JacketMatcher`), 추천 엔진(`Recommender`, `CompositeRecommender`). | `overmax_core`, `overmax_cv` |
| `overmax_engine` | `rust/overmax_engine` | 화면 캡처 엔진(DXGI, GDI, XComposite), 윈도우 추적기(`WindowTracker`), 템플릿 매칭(`templates/`), CV 디텍션 파이프라인(`DetectionPipeline`) 및 상태 머신(`PlayState`). | `overmax_core`, `overmax_cv`, `overmax_data` |
| `overmax_runtime` | `rust/overmax_runtime` | UI 없는 런타임: 감지 결과 분배 허브(`hub::Runtime` — 결과창 기록 스냅샷·PB 판정 → 기록 저장 → `RuntimeObserver`), IPC 서버·관찰자, 추천 계산(`derived`), V-Archive 업로드 판단·백그라운드 작업(`upload_plan`, `varchive_jobs`), 자동 업로드 스케줄러. | `overmax_core`, `overmax_data` |
| `overmax_app` | `rust/overmax_app` | 앱 진입점(`main.rs`), winit/egui UI 런타임, 다중 뷰포트 관리, Windows/Linux OS 연동(DPI V2, Z-Order, 네이티브 창 드래그, 단일 인스턴스 락, 자동 업데이트), 런타임 결과의 화면 반영과 다국어 문구 조립. | `overmax_core`, `overmax_cv`, `overmax_data`, `overmax_engine`, `overmax_runtime` (최상위) |

---

## 2. 런타임 스레드 모델

Overmax는 UI 렌더링 지연과 게임 내 프레임 드랍을 방지하기 위해 3개의 주요 스레드 그룹으로 작업을 분리한다.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          1. Main UI Thread                              │
│  - winit Event Loop & egui Render Pipeline (Overlay, Sync, Settings)    │
│  - 뷰포트 레이아웃 계산 (Auto-Fit Height, Native Window Drag)           │
│  - mpsc 채널을 통한 DetectionOutput 수신 및 UI 상태 반영                │
└───────────────────▲─────────────────────────────────▲───────────────────┘
                    │ (mpsc: DetectionOutput)         │ (mpsc: FetchResult)
┌───────────────────┴─────────────────┐   ┌───────────┴───────────────────┐
│     2. Detection Worker Thread      │   │    3. Background I/O Pool     │
│  - 주기적 화면 캡처 (DXGI/GDI)      │   │  - StartupCacheManager 동기화 │
│  - 씬 판별 & ROI 템플릿 매칭        │   │  - V-Archive 1곡 비동기 업로드│
│  - N-Frame 검증 및 원자적 Commit   │   │  - 외부 추천 Provider HTTP    │
│  - Local SQLite RecordDB 기록       │   │  - 앱 자동 업데이트 검사      │
└─────────────────────────────────────┘   └───────────────────────────────┘
```

### 1) Main UI Thread
* `eframe::run_native` 기반으로 실행되는 단일 메인 스레드.
* 오버레이(투명 윈도우), V-Archive 동기화 창, 설정 다이얼로그의 렌더링 루프를 처리한다.
* 동기 I/O나 무거운 이미지 연산을 일체 수행하지 않으며, 디텍션 워커와 I/O 워커로부터 전달받은 채널 메시지만 비동기로 소비한다.

### 2) Detection Worker Thread (`rust/overmax_engine/src/detector/detection_worker.rs`)
* 게임 창 활성화 상태와 씬 상태에 따라 가변 주기(16ms ~ 1000ms)로 루프를 수행하는 전용 워커 스레드.
* 화면 캡처 → 씬 감지 → ROI 매칭 → `PlayContext` 검증 → SQLite DB 기록 순으로 실행된다.
* 매 틱마다 산출된 `DetectionOutput`(게임 세션 상태, 윈도우 물리 좌표 `game_rect`/`window_snapshot`, 자켓 매칭 결과, 캡처 오류 등)을 `std::sync::mpsc::Sender<DetectionOutput>`을 통해 메인 UI 스레드로 전송하고 `request_repaint()` 콜백을 트리거한다.

### 3) Background I/O Pool
* 네트워크 호출 및 디스크 I/O가 메인 렌더 루프를 블로킹하지 않도록 `std::thread::spawn`을 통해 비동기 작업 단위로 격리 실행된다.
* **`StartupCacheManager`**: 부팅 시 캐시 파일 유무에 따라 비동기 백그라운드 갱신을 수행하고, 완료 시 채널로 새 `VArchiveDB`를 전달하여 메인 스레드에서 `Arc`를 재바인딩한다.
* **`VArchiveUploadWorker`**: 곡 플레이 종료 시 단일 곡 기록을 V-Archive로 전송하고 로컬 캐시를 갱신.
* **`RecommendProviderFetchWorker`**: 외부 HTTP 추천 API를 주기적/이벤트 기반으로 조회하여 캐시 파일에 기록.

---

## 3. 크로스 스레드 통신 흐름

스레드 간 통신은 채널 기반 단방향 메시지 패싱과 공유 뮤텍스(`SharedSettings`)를 사용한다.

```
[ Detection Worker ] ──(mpsc::Sender<DetectionOutput>)──► [ Main UI Thread ]
                                                                │
[ Main UI Thread ]   ──(mpsc::Sender<SyncEvent>)      ──► [ Sync Window ]
                                                                │
[ Main UI Thread ]   ──(mpsc::Sender<FetchReq>)       ──► [ Provider Fetcher ]
                                                                │
[ Provider Fetcher ] ──(mpsc::Sender<FetchResp>)      ──► [ Main UI Thread ]
```

1. **디텍션 상태 및 윈도우 스냅 전파**:
   디텍션 워커는 매 프레임의 `DetectionOutput`을 전송하며, 메인 UI는 매 프레임 `try_recv()`로 최신 스냅샷을 소비하여 오버레이 위치 이동과 상태 렌더링에 반영한다.
2. **동기화 이벤트 전파**:
   동기화 창(`SyncWindow`) 조작 시 발생하는 업로드 요청 및 결과는 `RecordKey`와 성공 여부를 담은 구조화된 이벤트를 통해 전달되어 UI 인덱스 꼬임을 방지한다.
3. **설정 동기화 (`SharedSettings`)**:
   설정값은 `Arc<Mutex<serde_json::Value>>`로 관리되며, 메인 UI 스레드와 디텍션 워커가 락을 획득하여 최신 설정을 참조한다. UI 조작 시의 디스크 저장은 백그라운드 스레드로 위임되어 렌더링 프레임 드랍을 방지한다.
4. **캐시 무중단 갱신**:
   `StartupCacheManager`가 비동기 갱신을 마치면 `mpsc`를 통해 메인 UI로 결과를 전송하고, 메인 스레드 렌더 루프(`poll_updates`)에서 `*varchive_db = Arc::new(new_vdb)`로 스마트 포인터를 교체한다.

---

## 4. 앱 수명주기 및 실행 흐름

```
[1. 앱 기동 (main)]
    │
    ├─► 단일 인스턴스 락 획득 (Windows: CreateMutexW / Linux: flock)
    ├─► Per-Monitor DPI Aware V2 선언
    ├─► settings.user.json 로드 (delta 머지)
    └─► StartupCacheManager 기동 (Cold Start 동기 / Warm Start 비동기)
    │
[2. 백그라운드 워커 시작]
    │
    ├─► DetectionWorker 스레드 생성 (CaptureEngine 초기화)
    └─► RecommendProviderFetch 스레드 대기
    │
[3. UI 렌더 루프 (eframe)]
    │
    ├─► winit 이벤트 수신 및 윈도우 위치/스케일 조정
    ├─► DetectionWorker로부터 GameState 수신
    ├─► 오버레이 패널 렌더링 (곡 정보, 레이팅, 추천곡)
    └─► 사용자 조작에 따른 보조 뷰포트(Sync, Settings) 렌더링
    │
[4. 앱 종료]
    │
    ├─► Worker 스레드 종료 시그널 전달 (AtomicBool stop flag)
    ├─► 변경된 설정 파일 디스크 flush
    └─► OS 리소스 및 단일 인스턴스 락 해제
```
