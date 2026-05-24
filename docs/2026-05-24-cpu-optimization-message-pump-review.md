# 역할(Role)
너는 Rust와 에셋 기반 UI 프레임워크(egui/eframe)에 정통한 시니어 시스템 개발자이자 소프트웨어 최적화 전문가이다.

# 목적(Objective)
이전 코드 수정 내역(Diff)에서 발견된 치명적인 버그(런타임 화면 미갱신) 및 로그 누락 위험을 해결하기 위해 추가적인 정밀 리팩토링을 수행하라. 불필요한 `request_repaint()`를 억제하여 CPU 사용률을 최소화하되, 상태 변화와 에러 이벤트는 완벽하게 화면에 반영되어야 한다.

# 작업 지시 사항 (Detailed Instructions)

## 1. `rust/overmax_app/src/detection_worker.rs` 수정
- **현상 및 문제점:** 이전 수정에서 자켓 매칭 상태의 변화를 감지하기 위해 `std::mem::discriminant(&out.jacket_status)`를 비교하도록 구현되었다. 하지만 `JacketMatchStatus` 열거형은 내부에 `song_id`나 `image_id` 같은 내부 연관 데이터(Associated Data)를 포함하고 있다. `discriminant`는 배리언트의 종류만 판단하므로, 곡이 변경되어도 동일한 `Matched` 배리언트라면 상태 변화를 감지하지 못해 UI가 갱신되지 않는 심각한 버그가 존재한다.
- **요구사항:**
  - `out.jacket_status`와 `self.last_jacket_status`를 매칭하여 내부 데이터(`song_id`, `image_id`)가 실제로 변경되었는지 개별적으로 비교하는 논리를 작성하라.
  - 배리언트 자체가 달라지거나 내부 아이디가 변경되었을 때만 `jacket_changed = true`가 되도록 하라.
  - 이를 `state_changed` 판단식에 결합하여, 실질적인 데이터 변경이 있을 때만 `self.request_repaint()`를 호출하도록 코드를 안전하게 수정하라.

## 2. `rust/overmax_app/src/native_app.rs` 수정
- **현상 및 문제점:** `spawn_fetch` 비동기 스레드 함수 내부에서 버튼별 동기화 루프 외부로 `ctx.request_repaint()`를 이동시켜 중복 호출을 줄인 것은 좋으나, `any_success == true` 조건문 내부에 갇혀 있다. 만약 네트워크 에러나 API 실패로 모든 요청이 실패하면 `ctx.request_repaint()`가 호출되지 않는다. 이로 인해 채널(`tx.send`)에는 에러 로그 메시지가 쌓였음에도 UI 스레드가 깨어나지 않아 사용자가 상호작용하기 전까지 에러 로그가 화면에 출력되지 않는 현상이 발생한다.
- **요구사항:**
  - `for b in buttons` 루프가 종료되는 시점에 성공 여부와 관계없이 **무조건** `ctx.request_repaint()`를 호출하도록 수정하라. 비동기 스레드가 종료되면서 채널에 메시지(성공이든 에러든)가 쌓였으므로, UI 스레드를 깨워 드레인(Drain)을 강제해야 한다.

---

# 기대하는 코드 변경 예시 (Reference Implementation Guide)

### [수정 가이드 1] `detection_worker.rs`
```rust
let jacket_changed = match (&out.jacket_status, &self.last_jacket_status) {
    (JacketMatchStatus::Matched { song_id: id1, .. }, JacketMatchStatus::Matched { song_id: id2, .. }) => id1 != id2,
    (JacketMatchStatus::InvalidId { image_id: id1, .. }, JacketMatchStatus::InvalidId { image_id: id2, .. }) => id1 != id2,
    (s1, s2) => std::mem::discriminant(s1) != std::mem::discriminant(s2),
};

let state_changed = out.current_song_id != self.last_song_id
    || out.is_song_select != self.last_is_song_select
    || out.logo_detected != self.last_logo_detected
    || jacket_changed;

self.last_song_id = out.current_song_id;
self.last_is_song_select = out.is_song_select;
self.last_logo_detected = out.logo_detected;
self.last_jacket_status = out.jacket_status.clone();

let _ = self.detection_tx.send(out);
if state_changed {
    self.request_repaint();
}

```

### [수정 가이드 2] `native_app.rs`

```rust
// for b in buttons { ... } 루프 종료 후
        }
        
        // 채널에 에러 혹은 성공 메시지가 전송 완료되었으므로 안전하게 리페인트를 호출하여 깨움
        ctx.request_repaint();
    });

```

---

# 수락 기준 (Acceptance Criteria)

1. 변경 후 `cargo check` 및 `cargo build` 시 컴파일 에러나 컴파일 경고(Warning)가 없어야 한다.
2. 곡이 변경되거나 자켓 매칭 결과 상태(Matched, InvalidId 등)가 바뀔 때 오버레이 UI가 즉각 반영되어야 한다.
3. 비동기 Fetch 도중 에러가 발생했을 때 사용자가 마우스를 움직이지 않아도 디버그 로그 창에 즉시 에러 로그가 업데이트되어야 한다.
4. 불필요한 상시 Repaint가 발생하지 않아 프로세스가 Idle 상태일 때 CPU 점유율이 0%대에 가까워야 한다.

준비가 되었으면 위 요구사항에 맞게 코드를 수정하고 결과를 보여줘.