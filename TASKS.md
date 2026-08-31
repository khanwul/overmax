# TASKS

Overmax 활성 작업 목록 및 마일스톤 로드맵입니다.  
(이전 완료 작업 목록은 [`docs/archive/tasks/TASKS_v0.4.0_archive.md`](docs/archive/tasks/TASKS_v0.4.0_archive.md)를 참조)

---

# 🚀 [Active] Milestone v0.4.1 — 외부 연동 프로토콜 및 배포 채널 확장 (IPC & MSIX / Store)

## 1. 외부 연동 및 IPC 프로토콜 고도화 (IPC & Extensibility Protocol)

- [x] **1.1 내부 ➔ 외부 이벤트 스트리밍 (Event Stream Broadcast)**
  - [x] 씬 전환, 곡 변경, 플레이 상태 및 결과 확정 이벤트의 실시간 IPC 브로드캐스트 (SSE 단일 포트 트랜스포트)
- [x] **1.2 외부 ➔ 내부 호출 인터페이스 (Inbound RPC / MCP 지원)**
  - [x] 외부 도구 및 AI 에이전트 연동을 위한 호출 프로토콜 설계 (JSON-RPC 2.0 및 MCP 연동 기반)
  - [x] 현재 곡 정보 조회, 추천 목록 요청, 오버레이 상태 제어 RPC 엔드포인트 정의
- [x] **1.3 Recommend-Provider 프로토콜 통합 및 정리**
  - [x] 기존 Provider Fetch 규격(`recommend-provider-protocol.md`)을 신규 IPC/RPC 아키텍처와 일관되게 단일화 및 정리

---

## 2. 데이터 저장소 및 런타임 환경 분기 (Storage & Runtime Environment)

- [ ] **2.1 데이터 경로 추상화 레이어 구축**
  - [ ] Portable 모드(바이너리 상대 경로) 및 Installed/MSIX 모드(`%LOCALAPPDATA%\Overmax\`) 듀얼 경로 지원
  - [ ] 사용자 설정(`settings.user.json`), 플레이 기록 DB(`cache/record.db`), 곡 메타 DB(`cache/songs.json`), 자켓 인덱스(`cache/image_index.db`)의 안전한 로드/마이그레이션 지원
- [ ] **2.2 스토어 환경 자가 업데이터 분기 처리**
  - [ ] MSIX 패키지 런타임 환경 감지(Win32 `GetCurrentPackageFullName` 또는 `store` feature flag)
  - [ ] 스토어 패키지 실행 시 인앱 자가 업데이터(GitHub Releases 바이너리 교체) 비활성화 및 UI 처리

---

## 3. MSIX 패키징 및 배포 파이프라인 (MSIX Packaging & Store CI)

- [ ] **3.1 Desktop Bridge 매니페스트 및 에셋 구성**
  - [ ] `AppxManifest.xml` 작성 (`runFullTrust` 권한, 패키지 Identity, 타일/로고 44x44/150x150 에셋 매핑)
  - [ ] MSIX 빌드/패키징 스크립트(`scripts/package-msix.ps1` 또는 `cargo-msix` / `MakeAppx` 연동) 구축 및 로컬 사이드로딩 검증
- [ ] **3.2 Microsoft Store 등록 및 CI 자동화**
  - [ ] Microsoft Partner Center 앱 등록 및 정책 대응 (서드파티 유틸리티 Disclaimer, 영/한 앱 설명, 스토어 스크린샷)
  - [ ] GitHub Actions 릴리즈 워크플로우에 MSIX 패키지 생성 및 Store 업로드/검수 파이프라인 연동

---

# 🎯 [Planned] Milestone v0.5.0 — 인게임 유틸리티 및 연동 고도화 (In-game Utilities & Automation)

## 4. 플레이어 편의성 및 인게임 유틸리티 (In-game Utilities & Controls)

- [ ] **4.1 글로벌/인게임 단축키(Hotkeys) 지원**
  - [ ] 오버레이 표시/숨김(Toggle Visibility), 라이트 모드 전환, 간편 V-Archive 업로드 단축키 연동
  - [ ] 게임 중 키 씹힘 방지 및 사용자 커스텀 단축키 설정 UI 제공
- [ ] **4.2 노트 레인 임시 가림막(Lane Blind / Curtain Overlay) 지원**
  - [ ] 연습용 상단/하단 가림막(SUDDEN / HIDDEN / BLIND 효과) 전용 경량 투명 서브 뷰포트 오버레이
  - [ ] 가림막 높이, 위치, 투명도 및 단축키 토글 제어 기능 지원

---

## 5. 기록 수집 및 V-Archive 자동 연동 (Record Automation)

- [ ] **5.1 결과창 V-Archive 자동 업로드 지원**
  - [ ] 결과창 씬 확정(`VerifiedPlayEvent`) 시 설정에 따라 백그라운드에서 V-Archive API 자동 업로드 트리거
  - [ ] 최고 기록(Personal Best) 갱신 시에만 선택적 자동 업로드 옵션 제공 및 네트워크 재시도 가드

---

## 6. 감지 씬 다양화 및 인게임 확장 (Scene Diversity & Ladder Match)

- [ ] **6.1 래더매치(Ladder Match) 씬 감지 대응**
  - [ ] 래더매치 밴픽/선곡 화면 및 대기실 감지 대응
  - [ ] 래더매치 결과창 인식 지원
