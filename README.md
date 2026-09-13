# Overmax

[한국어](README.md) | [English](README.en.md)

DJMAX RESPECT V 선곡 화면에서 V-Archive 기반 비공식 난이도 정보를 실시간으로 보여주는 오버레이 도구입니다.

> **🚀 Rust 네이티브 앱**: Overmax는 Rust 네이티브 기반으로 제작되어 가볍고 빠른 성능을 제공합니다.
> - **가볍고 빠른 성능**: 메모리 사용량과 실행 파일 크기가 최소화되었으며, 전반적인 런타임 성능이 뛰어납니다.
> - **외부 의존성 최소화**: 무거운 OpenCV나 OS OCR 의존성 없이, 순수 Rust 기반 Perceptual Hash/히스토그램 자켓 매칭과 Pure Rust CV 템플릿 매칭 엔진을 활용합니다.
> - **완벽한 하위 호환성**: 기존 사용자의 설정(`settings.json`) 및 로컬 기록(`record.db`)과 호환되며, 기존의 포터블(Portable) 환경을 그대로 유지합니다.

---

## 사용자 안내

### 무엇을 해 주나요?

선곡 화면에서 현재 선택된 곡의 **V-Archive 비공식 난이도**와 **지능형 맞춤 추천 목록**을 게임 화면 옆에 띄워줍니다.

- **비공식 난이도 실시간 표시**: 현재 선택 곡의 버튼 모드별 비공식 난이도 표시 (NM/HD/MX/SC)
- **지능형 다차원 추천 엔진**: 플레이어의 Top 50 실력 모델(TrueSkill SC/Pad 2-Track)을 분석하여 최적의 연습/도전 곡을 사유 뱃지(`BEST`, `RETRY`, `REST`, `PUSH` 등)와 함께 맞춤 추천
- **실시간 Rate / Max Combo 수집**: 순수 Rust 템플릿 매칭으로 점수와 레이트를 오차 없이 실시간 인식 및 로컬 저장 (실시간 신기록 감지 시 V-Archive 간편 업로드 지원)
- **초저지연 0.62ms GPU ROI Atlas 화면 캡처**: 512×512 아틀라스와 더블 버퍼링으로 인게임 끊김(Stuttering) 없는 서브밀리초 반응 속도 제공
- **Windows HDR(scRGB) 무설정 자동 감지**: 모니터 색역 및 SDR 백색 레벨을 자동 감지하여 64KB 고속 역변환 LUT를 통한 왜곡 없는 색상 복원 (DXGI 캡처)
- **실시간 로컬 IPC 스트리밍 및 원격 제어**: OBS 방송 위젯이나 서드파티 도구에서 실시간 게임 상태 수신(SSE `GET /events`) 및 JSON-RPC 2.0 원격 호출 연동 지원
- **라이트 모드 (Lite Mode)**: 화면 가림을 최소화하는 콤팩트 레이아웃(세로 높이 약 60px) 및 흔들림 없는 모서리 자동 스냅 제공
- **글로벌 다국어 지원**: 한국어, English, 日本語 3개 국어 UI 및 OS 표시 언어 자동 감지

메모리 읽기나 게임 프로세스 인젝션은 일절 없으며, **창 추적 + 화면 캡처** 방식으로만 안전하게 동작합니다.

### 설치 방법

#### Windows

1. [Releases](https://github.com/orphera/overmax/releases) 에서 최신 버전의 `overmax.zip`을 다운로드합니다.
2. 압축을 풀고 `overmax.exe`를 실행합니다. (포터블 모드와 설치형 모드 `%LOCALAPPDATA%\Overmax` 완벽 호환)
3. 실행 중 DJMAX RESPECT V를 실행하면 자동으로 인식이 시작됩니다.

> **자동 업데이트**: 앱 시작 시 자동으로 최신 버전 여부 및 곡 DB(`image_index.db`) 상태를 확인하여 업데이트를 수행합니다.

#### Linux (초기 지원)

1. Releases에서 `overmax-linux-x86_64.tar.gz`를 다운로드해 사용자 쓰기 가능한 디렉터리에 풉니다.
2. 해당 디렉터리에서 `./overmax`를 실행합니다. 설정과 캐시는 실행 디렉터리에 저장됩니다.
3. 같은 세션에서 Proton/XWayland로 DJMAX RESPECT V를 실행합니다.

지원 범위, 환경 확인 방법, 현재 구현 상태와 미지원 기능은 [Linux 지원 안내](docs/guides/linux-support.md)를 확인해 주세요.

### 요구사항

- Windows 10 이상 (64bit), 또는 위 초기 지원 범위를 만족하는 x86_64 Linux
- DJMAX RESPECT V (Steam)
- 실행 중 인터넷 연결 (V-Archive 데이터, DB 및 앱 업데이트 확인)

> ⚠️ **중요: 게임 화면 설정 안내**
> * **테두리 없는 전체화면(전체 창 모드) 권장**: 오버레이 창을 게임 화면 위에 정상적으로 띄워놓고 플레이하려면 게임 옵션에서 화면 설정을 **"전체 창 모드(Borderless Fullscreen)"**로 설정해 주세요.
> * **독점 전체화면 사용 시**: 게임을 일반 **"전체화면"** 모드로 실행하면 Windows OS 및 게임 안티치트(XIGNCODE3) 제약으로 인해 오버레이가 게임 위에 그려지지 못하고 게임 뒤로 숨게 됩니다. 독점 전체화면을 반드시 사용하셔야 하는 경우, 오버레이 창을 드래그하여 **듀얼 모니터의 보조 화면** 등 다른 모니터 영역에 배치해 두고 사용하셔야 합니다.

> **참고**: 오버레이 UI는 한국어, 영어, 일본어 다국어(i18n)를 지원하며, 설정 창에서 언제든지 언어를 변경할 수 있습니다.

### 설정

- 오버레이 헤더의 **톱니바퀴 버튼(⚙)**을 누르면 설정 창이 열립니다.
- 설정 창에서 **오버레이 크기(S / M / L / XL)**와 **투명도**, **표시 언어(한국어 / English / 日本語)**를 조절할 수 있습니다.
- 오버레이는 마우스 드래그로 원하는 위치에 자유롭게 옮길 수 있으며, 위치는 자동으로 저장됩니다.
- 설정 창에서 **라이트 모드**를 활성화할 수 있습니다. 라이트 모드 활성 상태에서는 의도치 않은 드래그 이동이 차단되며, 설정된 화면 구석 위치(좌상단, 우상단, 좌하단, 우하단)로 오버레이가 흔들림 없이(Jitter-free) 자동 스냅 및 고정됩니다.
- **고급 설정**: 캡처 엔진(DXGI / GDI), GPU ROI Atlas 가속, 로컬 IPC 통신 활성화, 데이터 저장 경로 확인 및 폴더 열기를 지원합니다.

---

## 개발자 안내

### 빌드 및 실행

```bash
# Rust 설치 필요 (rustup)
cargo build --release -p overmax-app
./target/release/overmax-rs
```

### 프로젝트 구조 (Rust)

- `rust/overmax_app`: 메인 어플리케이션 (egui/winit 기반 네이티브 다중 뷰포트 UI, 설정/디버그 창, IPC 서버 및 이벤트 루프)
- `rust/overmax_engine`: 화면 캡처(DXGI GPU Atlas / GDI / X11), HDR 2-Anchor 역변환, 디텍션 파이프라인, 상태 머신 및 텔레메트리
- `rust/overmax_core`: 핵심 상태 모델(`VerifiedPlayEvent` 등) 및 공통 도메인 타입
- `rust/overmax_data`: 설정(`settings.user.json`), DB(SQLite `record.db`), 추천 엔진 및 V-Archive API 연동
- `rust/overmax_cv`: 순수 Rust 기반 이미지 처리 핵심 알고리즘 (Perceptual Hash, 히스토그램, 템플릿 매칭 엔진 등)

### 빌드 및 배포 스크립트

- `scripts/package-rust.ps1`: 포터블 배포용 `overmax.zip`, `release_manifest.json` 생성 자동화 스크립트
- `scripts/package-msix.ps1`: Windows Desktop Bridge(Centennial) 기반 Microsoft Store / MSIX 패키징 스크립트
- `scripts/package-linux.sh`: Ubuntu 22.04/glibc 2.35 ABI 기준 x86_64 Linux `tar.gz` 생성 및 smoke 검증

---

## 데이터 출처

- [V-Archive](https://v-archive.net)

---

## 향후 개발 목표 (v0.5.0 로드맵)

현재 Overmax는 차기 버전(v0.5.0) 개발을 위한 백로그에 따라 다음 목표를 중점적으로 추진하고 있습니다. 세부 현황 및 이슈 추적은 [TASKS.md](TASKS.md)를 참고해 주세요.

1. **플레이어 편의성 및 인게임 유틸리티 (In-game Utilities & Controls)**: 글로벌/인게임 단축키(Hotkeys) 지원, 연습용 노트 레인 임시 가림막(Lane Blind / Curtain Overlay) 지원
2. **기록 수집 및 V-Archive 자동 연동 (Record Automation)**: 결과창 씬 확정 시 V-Archive API 백그라운드 자동 업로드
3. **감지 씬 다양화 및 인게임 확장 (Scene Diversity & Ladder Match)**: 인게임 래더매치(Ladder Match) 밴픽 화면, 대기실 및 결과창 인식 지원

---

## 라이선스

MIT
