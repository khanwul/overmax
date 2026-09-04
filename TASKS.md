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

- [x] **2.1 데이터 경로 추상화 레이어 구축**
  - [x] Portable 모드(바이너리 상대 경로) 및 Installed/MSIX 모드(`%LOCALAPPDATA%\Overmax\`) 듀얼 경로 지원
  - [x] 사용자 설정(`settings.user.json`), 플레이 기록 DB(`cache/record.db`), 곡 메타 DB(`cache/songs.json`), 자켓 인덱스(`cache/image_index.db`)의 안전한 로드/마이그레이션 지원

- [x] **2.2 스토어 환경 자가 업데이터 분기 처리**
  - [x] MSIX 패키지 런타임 환경 감지(Win32 `GetCurrentPackageFullName` 또는 `store` feature flag)
  - [x] 스토어 패키지 실행 시 인앱 자가 업데이터(GitHub Releases 바이너리 교체) 비활성화 및 UI 처리


---

## 3. MSIX 패키징 및 배포 파이프라인 (MSIX Packaging & Store CI)

- [x] **3.1 Desktop Bridge 매니페스트 및 에셋 구성**
  - [x] `AppxManifest.xml` 작성 (`runFullTrust` 권한, 패키지 Identity, 타일/로고 44x44/150x150 에셋 매핑)
  - [x] MSIX 빌드/패키징 스크립트(`scripts/package-msix.ps1` 또는 `cargo-msix` / `MakeAppx` 연동) 구축 및 로컬 사이드로딩 검증
- [x] **3.2 Microsoft Store 등록 및 CI 자동화**
  - [x] Microsoft Partner Center 앱 등록 및 정책 대응 (서드파티 유틸리티 Disclaimer, 영/한 앱 설명, 스토어 스크린샷)
  - [x] GitHub Actions 릴리즈 워크플로우에 MSIX 패키지 생성 및 Store 업로드/검수 파이프라인 연동

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

---

## 7. 캡처 파이프라인 고속화 (GPU ROI Atlas & Adaptive Normalization)

- [x] **7.1 컴파일 타임 아틀라스 레이아웃 & 트랜슬레이터 베이킹 (Step 1)**
  - [x] 43개 ROI(240,098 px)의 $512 \times 512$ 아틀라스 상수 배열(`pub const ATLAS_SLOTS: [AtlasSlot; 43]`) 베이킹 (`atlas_layout.rs`)
  - [x] `const fn get_roi_for_scene` 기반 $O(1)$ 정적 점프 테이블 트랜슬레이터 구현 (`atlas_translator.rs`)
  - [x] 기하학적 완전성 단위 테스트: 512×512 경계 검사 및 43개 슬롯 간 상호 AABB Overlap 0건 전수 검증
- [x] **7.2 오프라인 가상 아틀라스 CPU 무손실 검증 (Step 2)**
  - [x] 1080p 프레임을 $512 \times 512$ 가상 아틀라스로 조립하는 CPU 테스트 하네스 작성
  - [x] 가상 아틀라스 경유 디텍션 결과가 기존 파이프라인과 100% 일치함을 증명 (117개 테스트셋 및 `verify_pipeline`)
- [ ] **7.3 DXGI 1080p 순수 1:1 하드웨어 아틀라스 캡처 연동 (Step 3)**
  - [ ] 1080p 16:9 패스트패스: Draw Call 제로, 백버퍼 ➔ $512 \times 512$ VRAM Staging `CopySubresourceRegion` 43회 직행 (< 50 µs)
  - [ ] 단 1회 1MB `Map(D3D11_MAP_READ)` 전송 및 실측 0.72ms(1.9ms 절감) 레이턴시 검증
  - [ ] `settings.json` 안전 가드 플래그(`enable_gpu_atlas`) 및 예외 시 즉시 레거시 전체화면 캡처로 폴백 연동
- [ ] **7.4 비-1080p 조건부 GPU Normalizer 안전망 연동 (Step 4)**
  - [ ] 1080p가 아닐 때만 선별 동작하는 조건부 Draw Quad 렌더타겟($1920 \times 1080$) 파이프라인 구축
  - [ ] 16:10(Steam Deck), 21:9(울트라와이드), 1440p/4K UV Crop 및 하드웨어 Bilinear 리샘플링
  - [ ] 4K 환경에서 33MB 전송 폭탄 방지 및 아틀라스 무손실 전달 검증
- [ ] **7.5 자켓 64×64 GPU 프리리사이즈 & 360p/540p 초저해상도 한계 탐색 (Step 5)**
  - [ ] 자켓 매칭 1.65ms 절감을 위한 아틀라스 내 64×64 GPU 다운샘플링 패킹 최적화
  - [ ] 360p($640 \times 360$) / 540p 다운샘플링 환경에서 Rate 소수점(`.`) 및 Score 얇은 폰트 인식 한계 벤치마크

