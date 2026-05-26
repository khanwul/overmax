# Overmax v0.2.0 릴리즈 노트

> v0.1.5 이후 변경 사항

---

## 🦀 Rust 네이티브 앱으로 완전 재작성

가장 큰 변화입니다. 기존 Python 구현을 완전히 걷어내고 **순수 Rust 네이티브 앱**으로 처음부터 다시 작성했습니다.

- **실행 파일 크기 대폭 감소** — Python 인터프리터, OpenCV 런타임 등 무거운 의존성 제거
- **메모리 사용량 감소** — 네이티브 바이너리로 직접 실행
- **UI 프레임워크 전환** — PyQt6 → `egui` / `winit` (하드웨어 가속 멀티 뷰포트)
- **OpenCV 제거** — 이미지 처리(HOG, Perceptual Hash, 이진화 등)를 직접 구현한 `overmax_cv` 크레이트로 대체
- **기존 사용자 파일 100% 호환** — `settings.user.json`, `cache/record.db`, `cache/songs.json` 그대로 사용 가능

---

## ✨ 주요 신규 기능

### 인식 파이프라인

- **씬 인식 (Scene Detection)** — FREESTYLE / ONLINE 화면 자동 감지 및 씬별 ROI 자동 전환
  - 로고 OCR 멀티패스: Color → Grayscale → Binarized → Binarized Inverted 순으로 시도, 첫 매칭 시 즉시 반환
  - BGA 플래시, 복잡한 배경에 의한 오인식 대응 강화
- **Online 씬 지원** — 온라인 매칭 대기방에서도 곡/패턴 정보 표시
- **Rate OCR 멀티패스** — Color → Grayscale → Grayscale Inverted 3단계 시도로 배경 간섭에 강인한 인식
- **원자적 상태 안정화 (Atomic Play Context)** — 곡 ID, 버튼 모드, 난이도, Rate, MaxCombo가 N프레임 연속 동일하게 감지될 때만 기록 저장
- **히스테리시스 버퍼 (Hysteresis)** — 선곡 화면 진입/이탈 판정 오인식 방지
- **빠른 첫 자켓 매칭** — 선곡 화면 진입 직후 ~1초 이내 곡 인식 (이전 대비 4배 이상 개선)

### UI / UX

- **오버레이 크기 프리셋** — S / M / L / XL 4단계 버튼으로 즉시 변경
- **투명도 조절** — 슬라이더로 오버레이 투명도 실시간 조절
- **마우스 패스스루** — 오버레이 투명 영역 클릭이 게임 창에 그대로 전달
- **드래그 후 포커스 복원** — 오버레이 이동 후 자동으로 DJMAX 창에 포커스 반환
- **위치 저장** — 오버레이 드래그 종료 시 위치 자동 저장 및 다음 실행 시 복원
- **Steam 계정 자동 감지** — 로컬 Steam 설치에서 계정 목록 자동 파싱
- **Windows 트레이 아이콘** — 트레이에서 앱 상태 확인 및 종료
- **Alt-Tab 지원** — 설정/동기화/디버그 보조 창이 Alt-Tab 목록에 표시
- **새 앱 아이콘** — 투명 배경의 새 아이콘 적용 및 실행 파일에 직접 임베드

### 설정 / 동기화

- **V-Archive 계정 연동** — 파일 브라우저로 계정 파일 직접 지정 가능
- **V-Archive 기록 동기화 창 (Sync)** — 로컬에만 있는 기록 후보 스캔 및 개별 업로드/삭제 지원
  - 플레이 수 `MC` → `M`, 100% 기록 `P` 로 간결하게 표시
- **V-Archive 기록 자동 갱신** — 앱 시작 시 백그라운드에서 V-Archive 데이터 자동 패치
- **자동 업데이트** — GitHub Releases 기준으로 새 버전 확인 및 자동 업데이트

### 디버그

- **Debug UI** — 모듈별 실시간 로그, 카테고리 필터, 일시정지/지우기
- **Rate OCR 텔레메트리** — OCR에 전달된 실제 이미지(컬러/그레이스케일) 미리보기 + Threshold / BgMean / Invert 수치 실시간 표시

---

## 🔧 내부 개선

- `overmax_cv`: 자체 구현 HOG(자켓 매칭), Perceptual Hash, 이진화, 컬러/그레이스케일 OCR 전처리
- `overmax_data`: 설정 계층 관리(base / user delta), SQLite RecordDB, V-Archive API 클라이언트, 추천 정렬 로직
- `overmax_core`: 공통 상태 모델(`GameSessionState`, `PlayContext`, `SceneType`)
- 실행 파일 크기 최적화 — LTO 풀 빌드, `opt-level = 3`, panic=abort, 심볼 스트리핑 적용
- `⚠` 이모지 비교 `starts_with` 처리로 보조키 여부 파싱 안정성 향상

---

## 💾 하위 호환성

| 파일 | 호환 여부 |
|------|----------|
| `settings.user.json` | ✅ 완전 호환 |
| `cache/record.db` | ✅ 완전 호환 |
| `cache/songs.json` | ✅ 완전 호환 |
| `cache/image_index.db` | ✅ 완전 호환 |

---

## 📋 요구사항 변경

| 항목 | v0.1.x | v0.2.0 |
|------|--------|--------|
| 런타임 | Python 3.x + OpenCV | 없음 (네이티브 실행) |
| Windows 버전 | Windows 10 | Windows 10 1809 이상 (Windows OCR 필요) |
| 언어 설정 | 한국어/영어 필요 | 제한 없음 |
| UI 언어 | 한국어 | 한국어 |
