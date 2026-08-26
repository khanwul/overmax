# Overmax Recommend Provider Protocol (v1)

이 문서는 외부 커뮤니티 추천 서비스(행이봇, 로페봇, 디맥지지 등)가 자체 추천 로직을 HTTP 엔드포인트로 노출하여 **Overmax 인게임 오버레이 패널에 추천 곡을 제공**할 수 있도록 정의한 프로토콜 규격입니다.

---

## 1. 개요

Overmax는 외부 추천 결과의 **Viewer(소비자)** 역할만 담당합니다. 커뮤니티 서비스는 얇은 HTTP 규격에 맞춰 2개의 엔드포인트를 노출하기만 하면, Overmax 사용자가 알트탭(Alt-Tab) 없이 게임 오버레이 화면에서 맞춤형 추천 결과를 바로 확인할 수 있습니다.

* **Protocol Identifier**: `overmax-recommend/1`
* **통신 방식**: HTTP GET + JSON
* **캐시 및 갱신 방식**: 백그라운드 비동기 디스크 캐싱 (인게임 프레임 드랍 0%)

---

## 2. 엔드포인트 명세

### 2.1 Manifest 엔드포인트 (선택 권장)

Provider의 이름, 캐시 유효 시간(TTL), 반응 차원(`vary`), 추천 엔드포인트 경로 등 메타데이터를 선언합니다.

```http
GET /manifest
```

#### 응답 예시 (`200 OK`)

```json
{
  "protocol": "overmax-recommend/1",
  "name": "djmax.gg",
  "vary": ["mode"],
  "ttl_sec": 3600,
  "endpoint": "/recommend"
}
```

#### 필드 설명

| 필드 | 타입 | 설명 |
|---|---|---|
| `protocol` | string | **필수**. 고정 문자열 `"overmax-recommend/1"`. 다르면 무시됩니다. |
| `name` | string | 선택. Provider의 표시용 이름 (예: `djmax.gg`, `행이봇`). |
| `vary` | string[] | 추천 결과가 반응하는 컨텍스트 차원 배열. `["song_id", "mode", "diff", "v_id"]`의 부분집합.<br>• `[]` (빈 배열): 고정 추천 (예: "오늘의 추천곡"). 컨텍스트 변경 시 재요청하지 않음.<br>• 기본값: `["song_id", "mode", "diff"]`. |
| `ttl_sec` | number | 캐시 유효 시간 (초). 기본값: `3600` (1시간). |
| `endpoint` | string | 추천 요청 엔드포인트 경로 (상대/절대 경로 모두 가능). 기본값: `"/recommend"`. |

---

### 2.2 Recommend 엔드포인트 (필수)

실제 추천 곡 목록을 반환하는 엔드포인트입니다.

```http
GET {endpoint}?song_id={id}&mode={mode}&diff={diff}&v_id={v_id}
```

#### 요청 쿼리 파라미터

| 파라미터 | 타입 | 예시 | 설명 |
|---|---|---|---|
| `song_id` | number | `123` | V-Archive 곡 ID (로컬 `songs.json`의 `title` 식별자). |
| `mode` | string | `5B` | 버튼 모드: `"4B"`, `"5B"`, `"6B"`, `"8B"`. |
| `diff` | string | `SC` | 난이도: `"NM"`, `"HD"`, `"MX"`, `"SC"`. |
| `v_id` | string | `user123` | 사용자의 V-Archive ID (미설정 시 빈 문자열). |

#### 응답 예시 (`200 OK`)

```json
{
  "protocol": "overmax-recommend/1",
  "source": "djmax.gg",
  "entries": [
    {
      "song_id": 123,
      "mode": "5B",
      "diff": "SC",
      "reason": "유사 난이도 선호",
      "score": 0.87
    }
  ]
}
```

#### 필드 설명

| 필드 | 타입 | 설명 |
|---|---|---|
| `protocol` | string | **필수**. `"overmax-recommend/1"` 고정. |
| `source` | string | Provider 식별용 문자열. |
| `entries` | array | 추천 곡 엔트리 목록 (Overmax 화면에서는 상위 N개가 자라나 표시됨). |
| `entries[].song_id` | number | **필수**. V-Archive 곡 ID. |
| `entries[].mode` | string | **필수**. `"4B"`, `"5B"`, `"6B"`, `"8B"`. |
| `entries[].diff` | string | **필수**. `"NM"`, `"HD"`, `"MX"`, `"SC"`. |
| `entries[].reason` | string | 선택. 추천 사유 라벨 (예: `"인기곡"`, `"개인화 추천"`). |
| `entries[].score` | number | 선택. 추천 점수/우선순위 (0.0 ~ 1.0 권장). |

---

## 3. Overmax IPC(`overmax-ipc/1`)와의 관계

`overmax-recommend/1`은 **Provider(외부 서비스)가 구현하는 인바운드 규격**이고,
`overmax-ipc/1`(설정창 고급 탭의 로컬 IPC 서버)은 **Overmax가 구현하는 아웃바운드 규격**으로
역할이 다른 별개 프로토콜입니다. 다만 두 규격은 동일한 버저닝 문화와 공통 와이어 어휘를 공유합니다.

| 공유 요소 | 내용 |
|---|---|
| 버저닝 문화 | `x/1` 내 필드 추가는 호환, 키 변경은 `/2` 승격. 미지 필드는 수신자가 무시 (forward-compat) |
| 패턴 식별 어휘 | `song_id` / `mode` (`4B`,`5B`,`6B`,`8B`) / `diff` (`NM`,`HD`,`MX`,`SC`) — 이벤트·RPC·Provider 응답 모두 동일 |
| snake_case | 모든 JSON 필드명 |

Overmax 내부에서는 `RecommendEntry` 직렬화가 이 공통 어휘를 그대로 사용하며
(`rust/overmax_data/src/service/recommend/types.rs`의 계약 고정 단위 테스트 참조),
IPC RPC `get_recommendations` 응답도 동일한 키 체계로 제공됩니다.

---

## 4. 예제 파이썬 Mock 서버

프로토콜 구현을 검증할 수 있는 예제 Python Mock 서버가 `examples/recommend_mock_server.py` 파일로 제공됩니다.

### 실행 방법

```bash
python examples/recommend_mock_server.py
```

* 실행 시 `http://127.0.0.1:8080` 포트에서 대기합니다.
* Overmax 설정창 -> System 탭 -> **추천 Provider**에서 `http://127.0.0.1:8080`을 입력하여 연동을 테스트할 수 있습니다.

---

## 5. 프로토콜 식별자 상수

앱 내부에서는 문자열 리터럴 대신 상수를 사용해야 합니다 (불일치 방지):

* `overmax-recommend/1`: `rust/overmax_data/src/service/recommend_provider_fetch.rs`의 `RECOMMEND_PROTOCOL_ID`
* `overmax-ipc/1`: `rust/overmax_app/src/system/ipc_server.rs`의 `PROTOCOL_ID`
