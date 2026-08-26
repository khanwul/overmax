# IPC 서비스 아키텍처 철학 (v0.5.0 — 외부 연동 기반)

**작성일**: 2026-08-26  
**작업 브랜치**: `feat/ipc-service-architecture`  
**관련 TASKS**: v0.5.0 로드맵 1번 섹션 (IPC & Extensibility Protocol)  
**상태**: 철학 확정, 세부 결정 보류 (착업 시 채움)

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

### ③ 하나의 소켓, 하나의 언어 (Single Socket, Single Envelope)
단일 localhost 포트에서 이벤트 푸시(SSE)와 RPC(POST)를 모두 처리한다.
공용 엔벨로프 하나로 통일한다:

```json
{ "protocol": "overmax-ipc/1", "type": "event.play_verified" | "rpc.request" }
```

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
- **Named Pipe**: Windows 종속 — Linux 지원 범위와 충돌
- **tokio+axum**: 견고하나 무거운 의존성 트리 + 기존 std 스레드 아키텍처와 이질적

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

## 5. 열린 질문 (착업 시 결정)

1. **포트 정책**: 고정 vs 설정 가능, 충돌 시 fallback 여부
2. **기본 ON/OFF**: 아웃오브박스 켜짐 vs 설정에서 명시적 활성화
3. **엔벨로프 규격**: `overmax-ipc/1` 이벤트 스키마 및 버저닝 상세 (`overmax-recommend/1` 계승)
4. **RPC POST 경로 매핑**: MCP 시맨틱 매핑 위치 및 엔드포인트 설계 (1.2 착업 시)

---

## 6. Non-Goals

- 원격 노출 / 인증 체계 고도화
- 캡처 프레임 등 실시간 영상 스트리밍 (상태·이벤트만)
- 클라우드 동기화 등 서비스화 확장
