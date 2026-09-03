# UI and Overlay Runtime Architecture

이 문서는 Overmax의 UI 런타임(egui/winit), 멀티 뷰포트 분리, 투명 오버레이 렌더링 최적화, 0-Cost i18n 시스템 및 Linux Layer Overlay 아키텍처를 설명한다.

---

## 1. UI 런타임 개요 및 목표

Overmax의 UI 서브시스템은 `rust/overmax_app/src/ui/`에 위치하며 다음 기준을 만족하도록 구성되었다.

1. **게임 입력 방해 금지**: 오버레이는 최상위에 표시되되, 게임 포커스를 빼앗지 않고 키보드/컨트롤러 입력을 온전히 게임에 전달해야 한다.
2. **GPU 리소스 최소화**: egui 렌더링 파이프라인을 효율적으로 사용하여 GPU 사용률과 메모리 상주량을 최소 수준으로 유지한다.
3. **자연스러운 윈도우 조작**: 고주사율 모니터 환경에서도 부드럽게 창을 드래그하고, 내용물 높이에 따라 창 크기가 자동으로 맞춰져야 한다.

---

## 2. 멀티 뷰포트 아키텍처 (`native_app_viewports.rs`)

Overmax는 단일 OS 프로세스 내에서 `egui::ViewportId`를 활용하여 3개의 독립적인 윈도우(뷰포트)를 관리한다.

```
                         [ eframe::App (NativeApp) ]
                                      │
          ┌───────────────────────────┼───────────────────────────┐
          ▼                           ▼                           ▼
  [ Main Overlay Viewport ]   [ Sync Window Viewport ]   [ Settings Viewport ]
  - ViewportId::ROOT          - ViewportId("sync")       - ViewportId("settings")
  - 게임 화면 위 투명 오버레이 - V-Archive 동기화 창      - 시스템/인식/UI 설정 창
  - 클릭 무시 / 네이티브 드래그 - 모드/난이도/슬라이더 필터- 탭 기반 설정 UI
```

1. **메인 오버레이 (Root Viewport)**:
   * 투명 배경과 둥근 테두리를 가진 최상위 윈도우로, 선곡된 곡의 정보, 난이도별 레이팅, 추천 곡 목록, TOP 50 토스트 알림을 렌더링한다.
2. **동기화 창 (`SyncWindow`)**:
   * 사용자가 수동으로 V-Archive와 플레이 기록을 대조하고 일괄 업로드할 때 별도 윈도우로 생성된다.
   * 모드(4B~8B), 난이도(NM~SC), 달성률 슬라이더 등 다양한 필터 UI를 제공하며, 메인 오버레이 렌더링과 독립적으로 동작한다.
3. **설정 창 (`SettingsWindow`)**:
   * General, System, Display, About 등의 탭으로 구성된 표준 설정 다이얼로그이다.

---

## 3. 오버레이 렌더링 및 레이아웃 최적화

```
[ eframe Frame Start ]
        │
        ├─► 고정 폭(Width) 지정 (예: 280px, 320px)
        ├─► egui CentralPanel 내부 요소 렌더링 (곡 정보, 추천 목록)
        ├─► 최종 렌더링된 사각형 높이 측정: rect.height()
        │
        └─► measured_height != current_window_height 일 때만:
                SetWindowPos 또는 request_inner_size(Width, measured_height) 호출
```

### 1) 높이 동적 자동 맞춤 (Auto-Fit Height)
* 오버레이에 표시되는 추천 곡 수나 토스트 메시지 유무에 따라 창의 높이가 유동적으로 변한다.
* 이전 프레임의 창 크기가 뷰포트를 강제로 덮어써서 발생하는 크기 흔들림(피드백 루프)을 방지하기 위해, **Width는 고정하고 egui의 실제 렌더 응답 영역(`rect.height()`)에 맞춰 창 높이만 동적으로 갱신**한다.

### 2) OS 네이티브 윈도우 드래그 (`window_drag.rs`)
* 오버레이 상단 헤더 영역을 마우스로 잡고 이동할 때, egui 내부 마우스 좌표 계산 대신 OS 네이티브 창 드래그(`winit::window::Window::drag_window` 또는 Win32 `WM_SYSCOMMAND + SC_DRAGMOVE`)를 호출한다.
* 이를 통해 144Hz~240Hz 고주사율 모니터 환경에서도 마우스 커서와 오버레이 창 사이의 위치 딜레이나 끊김 현상 없이 부드럽게 창이 이동한다.

### 3) 투명 배경 렌더링 및 깜빡임 차단
* D3D/OpenGL 렌더 타깃의 Clear Color를 완전 투명(`Color32::TRANSPARENT`)으로 지정하고, 창 생성 시 Win32 알파 레이어드 속성을 적용하여 검은색 배경 깜빡임이나 잔상 없이 투명 오버레이를 유지한다.

---

## 4. 0-Cost 다국어(i18n) 시스템 (`i18n.rs`)

동적 문자열 할당과 런타임 맵 룩업 오버헤드를 없애기 위해 컴파일러 매크로 패턴 매칭 구조를 사용한다.

```rust
// 1. UI 코드에서의 호출
ui.label(t!("settings.general.language"));

// 2. 단일 top-level t! 매크로 디스패처
macro_rules! t {
    ("settings.general.language") => {
        $crate::t_select!(Ko => "언어", En => "Language")
    };
}

// 3. 로케일 기반 &'static str 직접 분기
macro_rules! t_select {
    (Ko => $ko:expr, En => $en:expr) => {
        match $crate::ui::i18n::current_locale() {
            $crate::ui::i18n::Locale::Ko => $ko,
            $crate::ui::i18n::Locale::En => $en,
        }
    };
}
```

1. **컴파일 타임 패턴 매칭 (`&'static str`)**:
   * 한국어(Ko)와 영어(En) 번역 텍스트는 매크로 정의부의 문자열 리터럴로 존재하며, 컴파일러의 `match` 분기를 통해 런타임 맵 룩업이나 힙 할당 없이 정적 참조(`&'static str`)를 반환한다.
   * 지원하지 않는 키를 호출하면 컴파일 타임 에러가 발생하여 오타를 사전에 방지한다.
2. **원자적 로케일 스위칭 (`AtomicU8`)**:
   * 설정창에서 언어를 변경하면 `static CURRENT_LOCALE: AtomicU8`에 원자적으로 저장(`Ordering::Relaxed`)되며, 다음 egui 렌더 프레임부터 즉시 새 언어가 반영된다.

---

## 5. Linux Wayland 네이티브 Layer Overlay (`linux_layer_overlay.rs`)

Linux 환경에서는 표준 X11 창 띄우기 방식 대신 Wayland 네이티브 프로토콜을 사용한다.

```
[ Wayland Compositor (Sway, Hyprland, niri 등) ]
                      │
       (wlr-layer-shell + optional
        wlr-foreign-toplevel 프로토콜)
                      │
                      ▼
          [ LayerSurface: OVERLAY ]
  - 게임 창 위 상위 레이어에 고정 배치
  - Keyboard Interactivity: None
  - 숨김 상태: empty input region으로 pointer/touch 통과
  - Fractional Scale & 다중 출력(wl_output) 자동 매핑
```

1. **`wlr-layer-shell` 연동**:
   * 오버레이를 `Layer::Overlay` 레벨로 생성하여 전체화면 게임 위에서도 가려지지 않고 최상단에 안정적으로 렌더링된다.
   * `KeyboardInteractivity::None`으로 설정하여 키보드 입력을 가로채지 않는다. 숨길 때는 surface 크기를 바꾸지 않고 투명 버퍼와 empty input region을 적용하며, 다시 표시할 때 기본 input region을 복구해 기존 드래그와 컨트롤을 유지한다.
2. **Fractional Scale 및 다중 출력 지원**:
   * 모니터별 Fractional Scale(예: 1.25배, 1.5배) 이벤트를 수신하여 렌더 버퍼 해상도를 동적으로 보정한다.
   * exact-title foreign-toplevel의 committed entered-output 집합에서 단일 output을 선택하고, 복수 output에서는 기존 선택을 유지한다. foreign-toplevel로 확정할 수 없으면 기존 X11 창과 Wayland output의 overlap 기반 선택으로 fallback한다.
   * output을 선택한 뒤 fullscreen 배치와 margin 계산에는 해당 output의 local logical geometry만 사용한다. 실제 output 교체만 surface를 재생성하고 동일 output의 geometry/scale 변경은 기존 surface에 반영한다.
3. **Optional foreign-toplevel 표시 상태**:
   * 동일한 Wayland 연결에서 `zwlr_foreign_toplevel_manager_v1`을 capability 기반으로 bind하고, 설정된 게임 제목과 완전 일치하는 단일 handle의 `done` 단위 activated/fullscreen 상태만 commit한다.
   * primitive 관찰값만 detection worker와 공유한다. protocol 부재, 중복 title, target close, manager 종료는 오류가 아니라 기존 X11/EWMH 3-state fallback으로 처리하며 Wayland proxy와 output 집합은 layer 스레드 밖으로 전달하지 않는다.
4. **상태 변경 wake와 latest-snapshot 전달**:
   * detection worker는 전체 `GameSessionState`와 engine-owned 표시 상태가 바뀔 때만 hidden eframe root를 깨운다. app은 기존 `same_display_snapshot` 비교를 거친 최신 snapshot만 non-blocking `UnixStream`으로 layer 스레드에 알린다.
   * debug/telemetry 빌드에서는 detection generation을 app drain, accepted publish, layer apply, present까지 이어 기록한다. 일반 release 빌드에는 이 계측 호출이 포함되지 않는다.
