# .antigravity/hooks/pre-save.ps1
$ErrorActionPreference = "Stop"

# 1. 변경된 파일 목록 수집
$allChangedFiles = git diff --name-only
$changedRustFiles = $allChangedFiles | Where-Object { $_ -like "*.rs" }
$changedDocsAndCode = $allChangedFiles | Where-Object { $_ -like "*.rs" -or $_ -like "*.md" }

# 2. 로컬 절대경로 사용 검사 (.md, .rs 파일 대상)
if ($changedDocsAndCode) {
    Write-Host "[Antigravity Hook] Checking for absolute local paths..." -ForegroundColor Cyan

    # 드라이브 문자로 시작하는 로컬 절대경로 패턴 (D:\dev\..., C:\Users\... 등)
    $violations = @()
    foreach ($file in $changedDocsAndCode) {
        if (Test-Path $file) {
            $matches = Select-String -Path $file -Pattern '[A-Z]:\\(dev|Users|home)\\' -AllMatches
            if ($matches) {
                $violations += $matches
            }
        }
    }

    if ($violations.Count -gt 0) {
        Write-Host "[Antigravity Hook] VIOLATION: Absolute local paths detected. Use relative paths instead:" -ForegroundColor Red
        foreach ($v in $violations) {
            Write-Host "  $($v.Filename):$($v.LineNumber): $($v.Line.Trim())" -ForegroundColor Yellow
        }
        Write-Error "[Antigravity Hook] Absolute local path violation! Use project-relative paths."
        exit 1
    }
}

# 3. Rust 코드 변경이 없으면 clippy/test 생략
if (-not $changedRustFiles) {
    Write-Host "[Antigravity Hook] No Rust file changes. Skipping clippy and tests." -ForegroundColor Green
    exit 0
}

Write-Host "[Antigravity Hook] Validating architectural constraints..." -ForegroundColor Cyan

# 4. 증분 캐시 기반의 cargo clippy 수행
cargo clippy --all-targets

if ($LASTEXITCODE -ne 0) {
    Write-Error "[Antigravity Hook] Complexity limit or layering rule violated! Rolling back changes."
    exit 1
}

# 5. 유닛 테스트 수행 (로직 회귀 방지)
cargo test --workspace

if ($LASTEXITCODE -ne 0) {
    Write-Error "[Antigravity Hook] Unit tests failed! Rolling back changes."
    exit 1
}

Write-Host "[Antigravity Hook] All checks passed successfully." -ForegroundColor Green
exit 0
