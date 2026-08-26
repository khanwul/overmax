# TASKS

Overmax v0.5.0 마일스톤 활성 작업 목록 및 백로그입니다.  
(이전 완료 작업 목록은 [`docs/archive/tasks/TASKS_v0.4.0_archive.md`](docs/archive/tasks/TASKS_v0.4.0_archive.md)를 참조)

---

## 1. 외부 연동 및 IPC 프로토콜 고도화 (IPC & Extensibility Protocol)

- [ ] **1.1 내부 ➔ 외부 이벤트 스트리밍 (Event Stream Broadcast)**
  - [ ] 씬 전환, 곡 변경, 플레이 상태 및 결과 확정 이벤트의 실시간 IPC 브로드캐스트 (WebSocket / SSE / Named Pipe 등)
- [ ] **1.2 외부 ➔ 내부 호출 인터페이스 (Inbound RPC / MCP 지원)**
  - [ ] 외부 도구 및 AI 에이전트 연동을 위한 호출 프로토콜 설계 (MCP - Model Context Protocol 유력 검토)
  - [ ] 현재 곡 정보 조회, 추천 목록 요청, 오버레이 상태 제어 RPC 엔드포인트 정의
- [ ] **1.3 Recommend-Provider 프로토콜 통합 및 정리**
  - [ ] 기존 Provider Fetch 규격(`recommend-provider-protocol.md`)을 신규 IPC/RPC 아키텍처와 일관되게 단일화 및 정리

---

## 2. 플레이어 편의성 및 인게임 유틸리티 (In-game Utilities & Controls)

- [ ] **2.1 글로벌/인게임 단축키(Hotkeys) 지원**
  - [ ] 오버레이 표시/숨김(Toggle Visibility), 라이트 모드 전환, 간편 V-Archive 업로드 단축키 연동
  - [ ] 게임 중 키 씹힘 방지 및 사용자 커스텀 단축키 설정 UI 제공
- [ ] **2.2 노트 레인 임시 가림막(Lane Blind / Curtain Overlay) 지원**
  - [ ] 연습용 상단/하단 가림막(SUDDEN / HIDDEN / BLIND 효과) 전용 경량 투명 서브 뷰포트 오버레이
  - [ ] 가림막 높이, 위치, 투명도 및 단축키 토글 제어 기능 지원

---

## 3. 기록 수집 및 V-Archive 자동 연동 (Record Automation)

- [ ] **3.1 결과창 V-Archive 자동 업로드 지원**
  - [ ] 결과창 씬 확정(`VerifiedPlayEvent`) 시 설정에 따라 백그라운드에서 V-Archive API 자동 업로드 트리거
  - [ ] 최고 기록(Personal Best) 갱신 시에만 선택적 자동 업로드 옵션 제공 및 네트워크 재시도 가드

---

## 4. 감지 씬 다양화 및 인게임 확장 (Scene Diversity & Ladder Match)

- [ ] **4.1 래더매치(Ladder Match) 씬 감지 대응**
  - [ ] 래더매치 밴픽/선곡 화면 및 대기실 감지 대응
  - [ ] 래더매치 결과창 인식 지원

---

## 5. 장기 백로그 (Long-term Backlog)

- [ ] **5.1 공식 V-Archive 클라이언트 보완/대체 올인원 파이프라인 (장기)**
  - [ ] 게임 플레이 종료 시 감지된 플레이 기록을 V-Archive API로 안전하게 자동 백그라운드 업로드하는 파이프라인 설계
