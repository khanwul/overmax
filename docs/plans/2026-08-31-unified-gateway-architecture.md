# 통합 Gateway & Transport 아키텍처 리팩토링 계획 (v0.5.0)

**작성일**: 2026-08-31  
**작업 브랜치**: `refactor/unified-gateway-architecture`  
**관련 마일스톤**: v0.5.0 (IPC & Extensibility Protocol 및 시스템 구조 건전성)  
**상태**: External Caller/Callee 전수 조사 완료 및 Transport 격리 계획 반영  

---

## 1. 배경 및 목적 (Motivation & Scope)

Overmax가 외부 세계(인터넷, 로컬 IPC, Steam, OS 서브시스템)와 상호작용하는 모든 **External Caller (우리가 외부를 호출)** 및 **External Callee (외부가 우리를 호출/관찰)** 영역을 전수 조사하고, 이를 깔끔하게 분리된 아키텍처로 정립합니다.

### 1.1 Overmax External Caller / Callee 전수 맵 (Total External Map)

```
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                                   Overmax Application                                  │
└───────────────┬────────────────────────────┬────────────────────────────┬──────────────┘
                │ [A] Web & Network          │ [B] Local IPC              │ [C] OS & Platform
                ▼                            ▼                            ▼
┌───────────────────────────────┐ ┌─────────────────────┐ ┌──────────────────────────────┐
│ 1. V-Archive REST & DB        │ │ 6. Local IPC Server │ │ 7. Steam Session & VDF/Save  │
│ 2. Google Sheets (서열표)     │ │    (SSE / JSON-RPC) │ │ 8. Single Instance Mutex     │
│ 3. GitHub (이미지 DB)         │ │                     │ │ 9. Desktop Shell (Browser)   │
│ 4. GitHub Self-Update         │ │                     │ │ 10. DXGI/GDI Screen Capture  │
│ 5. Custom Recommend Provider  │ │                     │ │ 11. Window Tracker (Win32)   │
└───────────────────────────────┘ └─────────────────────┘ └──────────────────────────────┘
```

#### [A] Outbound Web & Network (인터넷 통신)
1. **V-Archive API & DB**: 점수 업로드 (`POST`), 유저 기록 조회 (`GET`), 곡 DB `songs.json` 다운로드
2. **Google Sheets**: 4B/5B/6B/8B 난이도 서열표 CSV 다운로드
3. **GitHub Releases (이미지 DB)**: HOG 자켓 이미지 인덱스 DB 및 버전 파일 다운로드
4. **GitHub Releases (Self-Update)**: `self_update` 크레이트 기반 Overmax 바이너리 업데이트 확인 및 패치
5. **Recommend Provider**: `overmax-recommend/1` 프로토콜 기반 외부 추천 서버 조회

#### [B] Inbound & Real-time IPC (로컬 네트워크 통신)
6. **Local IPC Server**: `127.0.0.1:30110` 기반 SSE 이벤트 브로드캐스트 및 JSON-RPC 2.0 인바운드 제어

#### [C] OS & Platform Integration (로컬 OS/하드웨어 인터페이스)
7. **Steam 세션 & 세이브 파일 파싱**: Windows 레지스트리(`winreg`), Steam `localconfig.vdf`, DJMAX 세이브 파일시스템 직접 접근
8. **단일 인스턴스 락**: Win32 Named Mutex / Linux `flock`
9. **데스크톱 셸/브라우저 연동**: 시스템 브라우저 호출 (`open::that`), 탐색기 실행, Linux 폰트 조회(`fc-match`)
10. **화면 캡처 서브시스템**: Windows DXGI Output Duplication (D3D11) & GDI BitBlt
11. **윈도우 트래커**: Win32 `FindWindowW`, `GetWindowRect` 폴링

---

## 2. 이번 리팩토링의 핵심 스코프 (Refactoring Scope)

OS/캡처/Steam 등 로컬 플랫폼 연동([C])은 이미 각자 전용 모듈(`capture/`, `steam_session/`, `single_instance/`)로 잘 캡슐화되어 있습니다.  
따라서 이번 리팩토링은 **네트워크 I/O 영역([A] Web & Network 및 [B] Local IPC)의 Transport Layer 격리 및 Gateway 일원화**에 집중합니다.

---

## 3. 핵심 설계: Clean Transport & Gateway 분리

```
[ Domain / Service Layer ]
 ├─ Outbound Gateways (varchive, assets, provider) ──┐
 └─ Inbound IPC Service (Overmax RPC 메서드, IpcEvent) ──┼─── Overmax 비즈니스 로직만 집중
                                                        │
════════════════════════════════════════════════════════╪═════════════════════════════════════
[ Unified Transport Layer ]                             │
 ├─ Outbound: HttpClient (reqwest 래퍼, 커넥션 풀) ◄────┘
 └─ Inbound: LoopbackHttpServer (std TcpListener, SSE 엔진, JSON-RPC 라우터)
     • Overmax 도메인(DB, 곡 정보)을 일체 모르는 순수 네트워크 엔진
     • 소켓, HTTP 헤더 파싱, SSE 프레이밍, DNS Rebinding 가드 전담
```

### 3.1 Outbound Transport & Gateway
* **`overmax_data::gateway::http_client` (Transport)**:
  * 단일 `reqwest::blocking::Client` 인스턴스 관리 (Keep-Alive 커넥션 풀링)
  * 공통 User-Agent (`Overmax/{version}`) 및 목적별 타임아웃 프로파일 (Default: 10s, Fast: 3s, Download: 30s)
* **`overmax_data::gateway` (Domain Gateway)**:
  * `varchive.rs`: V-Archive 점수 업로드, 기록 조회, 곡 DB 다운로드
  * `asset_download.rs`: Google Sheets 서열표 CSV 및 GitHub 이미지 DB 다운로드
  * `recommend_provider.rs`: `overmax-recommend/1` 규격 외부 추천 호출

### 3.2 Inbound Transport & IPC Service
* **`overmax_app::system::transport::loopback_server` (Transport)**:
  * Overmax 도메인을 전혀 모르는 순수 std-only TCP/HTTP 서버
  * `TcpListener` 바인딩, 포트 대역 스캔, HTTP 요청 라인/헤더 파싱, `Host` DNS Rebinding 검증, Content-Type 가드
  * SSE 프레이밍 (`event: ...\ndata: ...\n\n`, `: ping\n\n`) 및 클라이언트 소켓 수명주기 관리
* **`overmax_app::system::ipc_server` (Domain IPC Service)**:
  * 885줄 ➔ 약 200줄로 극단적 단순화
  * JSON-RPC 메서드 구현 (`get_current_context`, `get_song_info`, `get_recent_plays`, `set_overlay_visibility`)
  * `IpcEvent` 정의 및 상태 스냅샷 조립

---

## 4. 단계별 실행 로드맵 (Phased Roadmap)

### Phase 1: Outbound Transport & Gateway 일원화
- [ ] `overmax_data::gateway::http_client` (통합 HTTP 클라이언트 엔진) 신설
- [ ] `varchive`, `asset_download`, `recommend_provider` 게이트웨이 구현
- [ ] 기존 호출부(`varchive_api.rs`, `client.rs`, `cache_downloader.rs`, `recommend_provider_fetch.rs`) 마이그레이션 및 레거시 제거
- [ ] `overmax_app/Cargo.toml`에서 `reqwest` 의존성 제거

### Phase 2: Inbound Transport 격리
- [ ] `overmax_app::system::transport::loopback_server` (순수 HTTP/SSE Transport 엔진) 분리 신설
- [ ] `ipc_server.rs`를 순수 비즈니스 RPC/이벤트 서비스로 리팩토링
- [ ] Transport 계층 독립 단위 테스트 작성

### Phase 3: 전체 통합 검증 및 문서화
- [ ] `cargo test --workspace` (단위 및 통합 테스트 100% 통과)
- [ ] `cargo clippy --all-targets` (경고 0개)
- [ ] V-Archive 동기화, 서열표 다운로드, Provider Fetch, IPC 연동 전체 E2E 회귀 검증
- [ ] `docs/decisions/data_and_sync.md`에 아키텍처 결정 기록 동기화
