# .antigravity/hooks/pre-save.ps1
$ErrorActionPreference = "Stop"

# 1. git diff로 변경된 Rust 파일(.rs) 목록 수집
$changedFiles = git diff --name-only | Where-Object { $_ -like "*.rs" }

if (-not $changedFiles) {
    # 변경된 Rust 코드가 없다면 패스
    exit 0
}

Write-Host "[Antigravity Hook] Validating architectural constraints..." -ForegroundColor Cyan

# 2. 증분 캐시 기반의 cargo clippy 수행
# Cargo.toml에 [workspace.lints.clippy]에서 level = "deny"로 설정한 룰들만 컴파일 에러로 간주되어 빌드가 실패합니다.
cargo clippy --all-targets

if ($LASTEXITCODE -ne 0) {
    Write-Error "[Antigravity Hook] Complexity limit or layering rule violated! Rolling back changes."
    exit 1
}

# 3. 유닛 테스트 수행 (로직 회귀 방지)
cargo test --workspace

if ($LASTEXITCODE -ne 0) {
    Write-Error "[Antigravity Hook] Unit tests failed! Rolling back changes."
    exit 1
}

Write-Host "[Antigravity Hook] All checks passed successfully." -ForegroundColor Green
exit 0

