# IPC 서비스 아키텍처 철학 (v0.5.0 — 외부 연동 기반)

**작성일**: 2026-08-26  
**작업 브랜치**: `feat/ipc-service-architecture`  
**관련 TASKS**: v0.5.0 로드맵 1번 섹션 (IPC & Extensibility Protocol)  
**상태**: 설계 전체 확정 (§3 트랜스포트, §5 규격, §6 구현 설계). 구현 진행 중

---

## 1. 목적과 범위

Overmax가 보유한 검증된 상태 전이(detection → stable commit → overlay/DB)를
외부 도구·AI 에이전트·대시보드가 구독하고 질의할 수 있도록 하는 **로컬 외부 연동 계층**의
설계 철학을 정의한다. 본 문서는 코드보다 먼저 합의된 원칙을 영속화하며, 세부 사양(포트, 스키마,
MCP 매핑)은 각 착업 단계에서 본 문서의 빈칸을 채우는 방식으로 발전시킨다.

### 1.1 대상 요구사항 (TASKS v0.5.0)

| TASKS | 요구 | 아키텍처 대응 |
|---|---|---|
| 1.1 | 씬 전환 · 곡 변경 · 플레이 결과 확정 이벤트 실시간 브로드캐스트 | SSE 푸시 채널 |
| 1.2 | 외부→내부 호출 (현재 곡 조회, 추천 목록, 오버레이 제어). MCP 유력 | POST RPC 엔드포인트 (MCP Streamable HTTP와 동형) |
| 1.3 | Provider Fetch 규격(`overmax-recommend/1`)과 신규 IPC/RPC의 일관화 | 프로토콜 버전 문화 단일화 (교체 아님) |

---

## 2. 설계 원칙 5계조

### ① 게임은 절대 기다리지 않는다 (Zero-Cost Idle)
모든 네트워크 I/O는 전용 스레드에서 발생하며 디텍션·렌더 경로와 완전히 분리된다.
구독 클라이언트가 0이면 리스너 accept 대기를 제외한 소켓 연산과 이벤트 팬아웃 비용이 0이다.
(성능 최우선 제약의 IPC 버전)

### ② 파이프라인은 관찰만 된다 (Read-Only Observation)
검증된 파이프라인(verified flow)은 수정하지 않는다. IPC 계층은 GUI 스레드가 이미 수신한
상태 전이(`detection_tx` 결과, `VerifiedPlayEvent` 확정)를 **구독**하는 형태로만 붙는다.
`is_stable = false`인 중간 상태는 외부에 노출하지 않는다 — 불변 조건 1번("stable일 때만
commit")의 자연스러운 확장.

### ③ 하나의 포트, 하나의 언어 (Single Port, Single Envelope)
단일 localhost 포트에서 이벤트 푸시(SSE)와 RPC(POST)를 모두 처리한다.
공용 엔벨로프 하나로 통일한다:

```json
{ "protocol": "overmax-ipc/1", "type": "play_verified", "seq": 42, "ts_ms": 1770000000000, "payload": { } }
```

RPC 요청은 같은 포트의 단일 `POST /rpc`에서 JSON-RPC 2.0 형식으로 운반한다.
기존 `overmax-recommend/1`의 버저닝 문화를 계승한다.

### ④ 로컬 신뢰, 명시적 동의 (Local-Only Trust)
`127.0.0.1` 바인딩 고정 — 원격 노출은 비목표(non-goal)이다. 화면 캡처 + OS API라는
비침투적 접근 방식과 같은 맥락으로, 외부 연동도 명시적인 네트워크 계약으로만 수행한다.

### ⑤ std-only 동기 스레드 (No Async Runtime, Zero Dependency)
async 런타임(tokio 등)을 도입하지 않고 `std::net::TcpListener` + thread-per-client로
구현한다. 신규 의존성 0개. TCP localhost는 Windows/Linux 공통이므로 플랫폼 분기가 없다.
동시 클라이언트 수는 소수 가정(1~5개).

---

## 3. 트랜스포트 결정 기록: SSE + 최소 HTTP 서버

### 3.1 최종 선택

**SSE(Server-Sent Events) + std-only 최소 HTTP 서버 (신규 의존성 0개)**

### 3.2 비교 평가 (결정 근거)

공통 전제: 우리는 **서버**(수신 측)이며, 기존 `reqwest`(blocking)는 클라이언트 전용이라
어느 쪽에도 도움이 되지 않는다. 어차피 1.2 RPC를 위해 만들어야 할 HTTP 서버 골격
(TcpListener + accept 루프 + 클라이언트 스레드, ~150–250줄)은 두 방식 모두 공통 필요분이다.

| 기준 | SSE | WebSocket (직접 구현) | WebSocket (tungstenite) |
|---|---|---|---|
| 신규 의존성 | **0개** | SHA-1/Base64까지 손구현 | 1 crate (+전이 의존성 ~10) |
| 프로토콜 계층 코드량 | **~30줄** (헤더+`data: ...\n\n`) | RFC 6455 프레이밍 상태머신 300–500줄 | 라이브러리 담당 |
| 양방향 RPC(1.2) | POST(RPC)+SSE(푸시) 표준 패턴 | 단일 소켓 자연스러움 | 동일 |
| 디버깅 도구 | curl -N 즉시 | websocat 등 전용 도구 | 동일 |
| 끊김 복구 | 재접속 하나로 수렴, EventSource 표준 자동 재접속 | close 경합/half-open 감지 등 에지 케이스 | 라이브러리 담당 |
| 안정성 책임 | HTTP 그대로 — 실패 모드 단순 | 손구현 시 우리 코드에 책임 전가 | 라이브러리 신뢰도 귀속 |

### 3.3 결정적 변수: 1.2의 MCP 호환

MCP(Model Context Protocol)의 공식 트랜스포트는 **stdio와 Streamable HTTP(POST + 선택적
SSE 응답)** 다. SSE+POST 조합은:

- MCP 표준 트랜스포트와 거의 1:1로 맞물려 1.2가 인프라 추가 없이 승격 가능
- WS는 MCP 표준 밖이라 모든 외부 에이전트가 커스텀 브릿지 작성 필요 → 생태계 호환성 구조적으로 깨짐

### 3.4 배제된 대안

- **WebSocket 직접 구현**: 손구현 프레이밍 400줄은 땜질이 아니라 장기 부채 (AGENTS.md
  땜질 금지 원칙 위반 소지)
- **tokio+axum**: 견고하나 무거운 의존성 트리 + 기존 std 스레드 아키텍처와 이질적

### 3.5 재검토 기록 (2026-08-26): "로컬 IPC" 관점 검증

최초 검토 당시 Named Pipe가 거론되었던 배경(외부 network 연동과의 거리감)에 대해
재검증한 결과, 기존 선택(SSE + loopback TCP)을 유지 확정한다.

**전제 사실 — fan-out은 트랜스포트와 직교이다**: 커널이 fan-out을 대행하는 크로스플랫폼
로컬 IPC 프리미티브는 존재하지 않는다(UDP 멀티캐스트가 유일한 커널 레벨 fan-out이지만,
방화벽·loopback 멀티캐스트 설정 편차로 기본값 채택 불가). TCP loopback / Named Pipe /
UDS 어느 쪽이든 N 클라이언트 팬아웃은 서버 프로세스(Broadcaster)의 몫이므로,
트랜스포트를 교체해도 fan-out 요구는 해소되지 않는다.

배제 근거:

- **Named Pipe(Win) + UDS(Linux) 듀얼 백엔드**: std는 UDS만 지원 — Windows Named Pipe는
  `Win32_System_Pipes` 기능 추가 + unsafe Win32 호출 필요. 백엔드 2개 유지에 따른
  플랫폼 분기와 CI 검증 행렬 2배는 Linux 호환 원칙("공용 계약 최소 확장")과 충돌.
  Linux FIFO는 다중 리더 시 데이터 분배가 비결정적이라 결국 UDS로 회귀. 대가 대비
  얻는 이득은 OS ACL 접근 제어뿐이며, 이는 토큰 방식(`cache/ipc_endpoint.json`에
  발급된 비밀을 헤더로 제시 — 파일 권한이 자연 게이트 역할)으로 아키텍처 변경 없이
  앱 레벨에서 동등 달성 가능한 업그레이드 경로가 된다.
- **공유메모리 링**: 알림 프리미티브가 OS마다 상이(eventfd vs named event)해 분기가
  불가피하고 슬로우 컨슈머 처리가 복잡. 초당 수 회·KB 미만인 본 서비스 이벤트 레이트에서
  성능 이득이 전무한 과설계.
- **loopback TCP의 성격 재확인**: `127.0.0.1` 한정 바인딩은 패킷이 NIC를 벗어나지 않는
  물리적 로컬 전용 채널이며, 명시적 loopback 바인딩이라 Windows 방화벽 팝업도
  발생하지 않는다.

---

## 4. 개념 구조

```
[Detection Worker] ──mpsc──▶ [GUI 스레드 (기존, 상태 소유자)]
                                   │ 상태 전이 스냅샷 구독 (논블로킹 try_send, 가득 차면 drop)
                                   ▼
                            [Broadcaster]          ← 신규 컴포넌트
                                   ▼
                            [IPC Server 스레드]     ← 신규 컴포넌트 (std TcpListener)
                                   │ SSE push / POST RPC (127.0.0.1)
                                   ▼
                        외부 도구 · AI 에이전트 · 대시보드
```

핵심: GUI 스레드 → Broadcaster는 **락 없는 논블로킹 송신**(채널 가득 시 drop),
IPC 서버 장애가 게임/오버레이에 무영향.

---

## 5. 확정 결정 (2026-08-26 2차 논의)

### 5.1 포트 정책: 대역 바인딩 + 설정 우선

- 기본 포트 `30110`, 허용 대역 **30100~30199**
  - 양 OS 임시(ephemeral) 포트 범위 밖 + 잘 알려진 점유자 없는 안전 대역
  - 회피 근거: 49152+(Windows 동적 포트), 32768~49151(Linux ip_local_port_range), 8080/3000/9000대(개발 도구 표준)
- `settings.json`의 `ipc.port`로 사용자 명시 설정 가능 (delta 형식 불변)
- 바인딩 실패 시 대역 내 `+1..+19` 순차 스캔 → 모두 실패하면 **IPC만 비활성화(fail-closed), 게임/오버레이 무영향**
- 실제 확정 포트는 로그 및 `cache/ipc_endpoint.json`(재생성 가능 캐시)으로 노출 — 향후 MCP stdio 브릿지의 포트 발견 경로

### 5.2 기본 OFF

- `ipc.enabled = false` 기본값. 원칙 ④(명시적 동의)의 직접 반영
- 설정창 **고급 탭**에 배치 (네트워크 노출 영역이므로 일반 플레이어 기본 동선에서 분리)

### 5.3 SSE 이벤트 규격: named-event + 단일 봉투

```text
GET /events  →
event: play_detected              ← SSE 표준 event 필드 (디스팟치용)
data: {"protocol":"overmax-ipc/1","type":"play_detected",
       "seq":42,"ts_ms":1770000000000,"payload":{...}}
```

- **연결별 단조 증가 `seq`**: 재접속 후 유실 감지용. 유실 시 RPC 스냅샷 재조회로 복구
- **접속 직후 `state_snapshot` 선송신**: 클라이언트가 POST 조회와 경쟁 없이 초기 상태 수령
- 하트비트: `: ping` 주석 행, 15초 고정 간격
- 버저닝: `/1` 내 필드 추가 허용, 호환 깨짐은 `/2`. 미지 이벤트 타입은 클라이언트가 무시 (forward-compat 의무화)
- 필드명은 `overmax-recommend/1`과 동일 snake_case (`song_id`, `mode`, `diff`) — 코어 타입(`VerifiedPlayEvent`)과 1:1 대응
- v1 이벤트 3종: `scene_detected`, `song_detected`, `play_verified`
  - `_detected` 명명: 상태 변화를 '선언'하는 게 아니라 관찰된 '감지'를 '통지'하는 입장 (원칙 ② 반영). 단, `play_verified`는 verified flow 용어를 그대로 계승

### 5.4 RPC 매핑: 단일 `POST /rpc`, JSON-RPC 2.0 서브셋

- 결정적 근거: MCP 자체가 와이어 포맷으로 JSON-RPC 2.0을 사용 → 1.2에서 `/mcp` 엔드포인트가 내부 핸들러 레지스트리를 공유하는 얇은 어댑터로 승격 가능
- 초기 메서드 3종: `get_current_context`, `get_recommendations`, `set_overlay_visibility` (읽기 우선, 제어 최소 1개)
- 라우트 3개로 고정: `/rpc`, `/events`, `GET /`(매니페스트 — 발견·디버깅용)
- 보호 가드 (비용 ≈ 0):
  - `Host: 127.0.0.1[:port]` 헤더 검증 → DNS 리바인딩 차단
  - `Content-Type: application/json` 강제 → CORS preflight 유도로 악성 웹페이지의 cross-origin POST 차단

---

## 6. 구현 설계 결정 (2026-08-26 3차 논의 — 착수 직전 확정)

### 6.1 스레드 모델: 리스너 + 허브 + 클라이언트당 스레드

```
[GUI drain 루프] ─try_send(bounded 64)→ [허브 스레드] ─Sender 클론 fan-out→ [SSE 클라이언트 스레드들]
[리스너 스레드: nonblocking accept, 250ms 폴링]
```

- **설정 변경 감지는 폴링**: `detection_worker`가 `merged_settings`(Arc&lt;Mutex&gt;)를 매 사이클 읽는 기존 패턴과 동일하게, accept 루프 250ms 틱마다 설정을 읽어 ON/OFF·포트 변경 시 리바인딩. 신규 명령 채널 없음 — 설정 변경이 앱 재시작 없이 반영됨
- **seq는 클라이언트 writer 스레드가 할당** (연결별 단조 보장)
- **백프레셔**: GUI→허브 bounded 채널 가득 시 해당 이벤트 drop (`state_snapshot`+RPC로 회수 가능). 게임/GUI 경로는 절대 블록되지 않음 (원칙 ①)
- 유휴 비용: 클라이언트 0명 시 park된 스레드 2개 + 폴링 4회/초 ≈ 0

### 6.2 이벤트 훅 지점: `drain_detection_results()` 단일 관찰자

파이프라인 수정 0 — GUI 계층의 `native_app_recommend.rs::drain_detection_results()`가
이미 필요한 3종 이벤트 원천을 모두 보유:

| 이벤트 | 원천 |
|---|---|
| `scene_detected` | `self.session != output.state` 비교 지점 |
| `song_detected` | 안정화된 컨텍스트 (`state.is_valid()`) |
| `play_verified` | `output.event` (DB 커밋과 동일 이벤트) |

중복 억제는 퍼블리셔 측 compare-and-publish(직전 발행 스냅샷과 동일하면 스킵)로 처리.

### 6.3 `state_snapshot` 페이로드: `GameSessionState` 직렬화 그대로 + 알파

```json
{ "scene": "Freestyle", "stable": true, "fullscreen": true,
  "context": { "song_id": 123, "mode": "5B", "diff": "SC",
               "rate": 99.23, "is_max_combo": false } | null,
  "app_version": "0.4.0" }
```

- 코어 타입(`GameSessionState`, `PlayContext`)이 이미 `Serialize` 파생이라 변환 코드 최소
- **곡 제목은 의도적 미포함**: `songs.json`은 공개 데이터이므로 클라이언트가 `song_id`로
  자체 해석. IPC↔VArchiveDB 결합을 만들지 않음 (관찰 최소주의, 원칙 ②)

### 6.4 설정 UI: 고급 탭 섹션 카드 1장

기존 `section_card` 패턴 준용 — enabled 체크박스 + 포트 입력(1024~65535 clamp,
권장 대역 30100~30199 힌트) + 런타임 상태 라인(`실행 중 · 127.0.0.1:{port}`).
i18n ko/en 키 추가. 저장은 기존 delta 디바운스 플로우 그대로.

---

## 7. Non-Goals

- 원격 노출 / 인증 체계 고도화
- 캡처 프레임 등 실시간 영상 스트리밍 (상태·이벤트만)
- 클라우드 동기화 등 서비스화 확장
