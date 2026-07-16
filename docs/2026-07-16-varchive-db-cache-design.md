# V-Archive 캐시 데이터베이스 통합 설계안 (Generated Columns 활용)

이 문서는 기존 파일 기반 JSON 캐시(`cache/varchive/*.json`) 데이터를 `record.db` (SQLite) 내부의 독립된 테이블로 통합할 때의 성능 오버헤드 문제를 해결하고 유연성을 확보하기 위한 하이브리드 데이터베이스 설계를 정의합니다.

---

## 1. 배경 및 쟁점
* **배경**: 기존 V-Archive 캐시 JSON 파일은 한 곡당 15개 이상의 다양한 통계 필드(`djpower`, `maxDjpower`, `rating`, `floorName`, `dlcCode` 등)를 보관하고 있습니다.
* **통합의 필요성**: 디스크 I/O 렉 방지 및 로컬 DB 기록과의 직접적인 비교(SQL JOIN)를 실현하기 위해 SQLite 통합이 필요합니다.
* **쟁점**: V-Archive의 모든 상세 필드를 개별 컬럼으로 분리하기에는 API 변경 시의 스키마 마이그레이션 비용과 데이터 중복(redundancy)이 큽니다. 그렇다고 JSON 전체를 텍스트 컬럼에 넣고 `json_extract`로 실시간 동적 파싱을 하게 되면 **CPU 런타임 연산 비용 증가와 인덱스 미적용에 따른 쿼리 성능 저하**가 발생합니다.

---

## 2. 해결 대안: 생성된 컬럼 (Generated Columns) 도입

원본 JSON 데이터를 온전히 `raw_data` 컬럼에 직렬화하여 담아두면서, 비교 및 쿼리에 필수적인 특정 필드(`score`, `maxCombo`)는 SQLite가 디스크에 쓸 때 미리 자동으로 파싱하여 물리적 컬럼으로 보관(`STORED`)하도록 설계합니다.

### 2.1 테이블 스키마 DDL 정의
```sql
CREATE TABLE IF NOT EXISTS varchive_records (
    steam_id      TEXT NOT NULL,
    song_id       TEXT NOT NULL,
    button_mode   TEXT NOT NULL,
    difficulty    TEXT NOT NULL,
    raw_data      TEXT NOT NULL, -- V-Archive API 레코드 JSON 객체 전체 (유연성 확보)
    
    -- raw_data JSON에서 자동으로 추출되어 디스크에 물리적으로 저장되는 컬럼 (SQLite 3.31.0+)
    score         REAL GENERATED ALWAYS AS (json_extract(raw_data, '$.score')) STORED,
    max_combo     INTEGER GENERATED ALWAYS AS (json_extract(raw_data, '$.maxCombo')) STORED,
    
    PRIMARY KEY (steam_id, song_id, button_mode, difficulty)
);

-- score 컬럼에 대해 인덱스를 적용하여 인게임 초고속 쿼리 성능 보장
CREATE INDEX IF NOT EXISTS idx_varchive_score ON varchive_records (score);
```

### 2.2 메타데이터 동기화 관리 테이블
동기화 주기와 데이터 일관성을 관리하기 위한 경량 메타 테이블을 병행 사용합니다.
```sql
CREATE TABLE IF NOT EXISTS varchive_sync_meta (
    steam_id      TEXT NOT NULL,
    button_mode   TEXT NOT NULL,
    last_sync_at  TEXT NOT NULL,
    PRIMARY KEY (steam_id, button_mode)
);
```

---

## 3. 기술적 타당성 검토
1. **SQLite 기능 내장**:
   Generated Columns 기능은 SQLite 3.31.0 이상, JSON 함수군(`json_extract`)은 3.38.0 이상에서 기본 탑재되어 있습니다.
2. **의존성 바인딩 (`rusqlite`) 호환성**:
   현재 프로젝트는 `rusqlite = { version = "0.31.0", features = ["bundled"] }`를 사용하여 빌드됩니다. `"bundled"` 피처로 인해 정적 컴파일 및 링크되는 내장 SQLite 엔진 버전은 **3.40.0 이상**이므로, 별도의 드라이버 업그레이드나 패키지 의존성 추가 없이 **즉각 100% 기능 사용이 가능**합니다.
3. **Rust 개발 생산성**:
   `rusqlite`를 이용해 데이터를 쿼리할 때, Rust 코드에서는 JSON 파싱이나 문자열 처리에 신경 쓸 필요 없이 네이티브 `f64` / `bool` 형식의 컬럼처럼 바로 조회 및 데이터 바인딩을 처리할 수 있어 구현이 매우 단순해집니다.
   ```rust
   // Rust 예시: 일반 컬럼처럼 조회
   let score: f64 = row.get("score")?;
   ```

---

## 4. 권장 마이그레이션 단계
향후 캐시 마이그레이션 작업 착수 시 다음 3단계 방안을 권장합니다.

1. **DB 테이블 빌드**: `RecordDB::initialize` 시점에 위 테이블 생성을 추가합니다.
2. **마이그레이션 브리지 실행**: 기존 로컬 JSON 캐시 디렉토리(`cache/varchive/*.json`)가 존재하면 파일 데이터를 한꺼번에 읽어 SQLite 테이블에 일괄 `INSERT` (Upsert) 하고 기존 원본 JSON은 백업 후 삭제합니다.
3. **RecordManager 전환**: `RecordManager::refresh`가 더 이상 디스크에서 JSON을 로딩하지 않고, `SELECT * FROM varchive_records WHERE steam_id = ?`로 조회해 간결한 인메모리 맵 형태로 캐싱하게 변경합니다.
