<#
.SYNOPSIS
    Overmax MSIX Packaging & Local Sideloading Script

.DESCRIPTION
    Builds the release binary, stages package contents, generates AppxManifest.xml,
    packs into an MSIX file using MakeAppx, signs with signtool (optional),
    and installs locally via Add-AppxPackage (optional).

.PARAMETER SkipBuild
    Skip running `cargo build --release`.

.PARAMETER StoreFeature
    Compile with `--features store` flag.

.PARAMETER Sign
    Sign the MSIX package using signtool.

.PARAMETER Install
    Install (sideload) the generated MSIX package onto the local system.

.PARAMETER Uninstall
    Uninstall the existing Overmax MSIX package from the local system.

.PARAMETER Publisher
    Package publisher identity. Defaults to "CN=OvermaxDev" for local development.

.PARAMETER PackageName
    Package identity name. Defaults to "Orphera.Overmax".

.PARAMETER CertPath
    Path to signing certificate PFX. If not specified and -Sign is passed,
    a local development certificate will be created/used.

.PARAMETER CertPassword
    Password for the PFX certificate. Defaults to "overmax".
#>

param(
    [switch]$SkipBuild,
    [switch]$StoreFeature,
    [switch]$Sign,
    [switch]$Install,
    [switch]$Uninstall,
    [string]$Publisher = "CN=OvermaxDev",
    [string]$PackageName = "Orphera.Overmax",
    [string]$PackageDisplayName = "Overmax",
    [string]$PublisherDisplayName = "hitel00000",
    [string]$PackageDescription = "DJMAX RESPECT V In-game Overlay & Recommendation Utility",
    [string]$CertPath = "",
    [string]$CertPassword = "overmax"
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

function Find-WindowsSdkTool {
    param([string]$toolName)
    $cmd = Get-Command $toolName -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }

    $sdkBins = "C:\Program Files (x86)\Windows Kits\10\bin"
    if (Test-Path $sdkBins) {
        $matches = Get-ChildItem -Path $sdkBins -Filter $toolName -Recurse -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -like "*\x64\*" } |
            Sort-Object { $_.FullName } -Descending
        if ($matches.Count -gt 0) {
            return $matches[0].FullName
        }
    }
    return $null
}

Push-Location $repoRoot
try {
    # Uninstall command
    if ($Uninstall) {
        Write-Host "Searching for installed package '$PackageName'..."
        $pkg = Get-AppxPackage -Name $PackageName -ErrorAction SilentlyContinue
        if ($pkg) {
            Write-Host "Removing package $($pkg.PackageFullName)..."
            Remove-AppxPackage -Package $pkg.PackageFullName
            Write-Host "Successfully uninstalled $PackageName."
        } else {
            Write-Host "Package $PackageName is not currently installed."
        }
        if (-not $Install -and -not $Sign -and $SkipBuild) {
            return
        }
    }

    # 1. Locate SDK Tools
    $makeAppx = Find-WindowsSdkTool "makeappx.exe"
    if (-not $makeAppx) {
        throw "makeappx.exe not found in PATH or Windows Kits. Please install Windows 10/11 SDK."
    }
    Write-Host "Using MakeAppx: $makeAppx"

    $signTool = $null
    if ($Sign -or $Install) {
        $signTool = Find-WindowsSdkTool "signtool.exe"
        if (-not $signTool) {
            throw "signtool.exe not found in PATH or Windows Kits. Required for signing."
        }
        Write-Host "Using SignTool: $signTool"
    }

    # 2. Build Release Binary
    $exe = Join-Path $repoRoot "target\release\overmax-rs.exe"
    if (-not $SkipBuild) {
        Write-Host "Building overmax-app --release..."
        $cargoArgs = @("build", "-p", "overmax-app", "--release")
        if ($StoreFeature) {
            $cargoArgs += @("--features", "store")
        }
        & cargo @cargoArgs
        if ($LASTEXITCODE -ne 0) {
            throw "Cargo build failed with exit code $LASTEXITCODE"
        }
    }

    if (-not (Test-Path $exe)) {
        throw "Binary not found at $exe. Please build first or remove -SkipBuild."
    }

    # 3. Extract Version
    $cargoTomlPath = Join-Path $repoRoot "Cargo.toml"
    $versionMatch = Select-String -Path $cargoTomlPath -Pattern '^version\s*=\s*"([^"]+)"'
    if (-not $versionMatch) {
        throw "Failed to extract version from Cargo.toml"
    }
    $rawVersion = $versionMatch.Matches.Groups[1].Value
    # SemVer -> MSIX 4-part version (Major.Minor.Build.0)
    # Microsoft Store policy strictly requires the revision number (4th digit) to be 0:
    # "Apps cannot use version numbers that specify a non-zero revision number in the app manifest."
    $cleanVersion = ($rawVersion -split '-')[0]
    $versionParts = $cleanVersion.Split('.')
    while ($versionParts.Count -lt 3) {
        $versionParts += "0"
    }
    $msixVersion = "$($versionParts[0]).$($versionParts[1]).$($versionParts[2]).0"
    Write-Host "Resolved MSIX Version: $msixVersion (Semver: $rawVersion)"

    # 4. Prepare Staging Directory
    $distDir = Join-Path $repoRoot "dist"
    $stagingDir = Join-Path $distDir "msix_staging"
    $msixOutput = Join-Path $distDir "Overmax-$rawVersion-x64.msix"

    if (Test-Path $stagingDir) {
        Remove-Item -Recurse -Force $stagingDir
    }
    New-Item -ItemType Directory -Path $stagingDir | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $stagingDir "Assets") | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $stagingDir "cache") | Out-Null

    # Copy binary
    Copy-Item $exe (Join-Path $stagingDir "overmax.exe") -Force

    # Copy default configs & docs if present
    if (Test-Path (Join-Path $repoRoot "settings.json")) {
        Copy-Item (Join-Path $repoRoot "settings.json") $stagingDir -Force
    }
    if (Test-Path (Join-Path $repoRoot "README.md")) {
        Copy-Item (Join-Path $repoRoot "README.md") $stagingDir -Force
    }

    # Copy bundled cache if present
    $cacheDir = Join-Path $repoRoot "cache"
    if (Test-Path $cacheDir) {
        $seedFiles = @("songs.json", "dlcs.json", "pattern_meta.json", "image_index.db")
        foreach ($f in $seedFiles) {
            $src = Join-Path $cacheDir $f
            if (Test-Path $src) {
                Copy-Item $src (Join-Path $stagingDir "cache") -Force
            }
        }
    }

    # Copy visual assets
    $assetsDir = Join-Path $repoRoot "packaging\msix\Assets"
    if (-not (Test-Path $assetsDir)) {
        throw "MSIX visual assets not found at $assetsDir. Please generate them first."
    }
    Copy-Item (Join-Path $assetsDir "*") (Join-Path $stagingDir "Assets") -Force

    # Generate AppxManifest.xml from template
    $templatePath = Join-Path $repoRoot "packaging\msix\AppxManifest.xml.template"
    if (-not (Test-Path $templatePath)) {
        throw "AppxManifest template not found at $templatePath"
    }

    $manifest = Get-Content -Path $templatePath -Raw -Encoding UTF8
    
    function Escape-XmlText([string]$str) {
        if (-not $str) { return "" }
        return [System.Security.SecurityElement]::Escape($str)
    }

    $manifest = $manifest.Replace("{{PACKAGE_NAME}}", (Escape-XmlText $PackageName))
    $manifest = $manifest.Replace("{{PACKAGE_PUBLISHER}}", (Escape-XmlText $Publisher))
    $manifest = $manifest.Replace("{{PACKAGE_VERSION}}", (Escape-XmlText $msixVersion))
    $manifest = $manifest.Replace("{{PACKAGE_DISPLAY_NAME}}", (Escape-XmlText $PackageDisplayName))
    $manifest = $manifest.Replace("{{PUBLISHER_DISPLAY_NAME}}", (Escape-XmlText $PublisherDisplayName))
    $manifest = $manifest.Replace("{{PACKAGE_DESCRIPTION}}", (Escape-XmlText $PackageDescription))

    $manifestDest = Join-Path $stagingDir "AppxManifest.xml"
    [System.IO.File]::WriteAllText($manifestDest, $manifest, [System.Text.Encoding]::UTF8)
    Write-Host "Generated AppxManifest.xml with Publisher '$Publisher' and Version '$msixVersion'"

    # 5. Pack MSIX with MakeAppx
    Write-Host "Packing MSIX with MakeAppx..."
    if (Test-Path $msixOutput) {
        Remove-Item -Force $msixOutput
    }

    & $makeAppx pack /d $stagingDir /p $msixOutput /o
    if ($LASTEXITCODE -ne 0) {
        throw "MakeAppx failed with exit code $LASTEXITCODE"
    }
    Write-Host "MSIX successfully created: $msixOutput"

    # 6. Signing (Optional or when Installing)
    if ($Sign -or $Install) {
        $pfxPath = $CertPath
        $pfxDir = Join-Path $repoRoot "packaging\msix"
        $cerPath = Join-Path $pfxDir "OvermaxDev.cer"

        if (-not $pfxPath) {
            # Check or create default dev certificate
            $pfxPath = Join-Path $pfxDir "OvermaxDev.pfx"
            if (-not (Test-Path $pfxPath)) {
                Write-Host "Creating self-signed development certificate for '$Publisher'..."
                $secPass = ConvertTo-SecureString $CertPassword -AsPlainText -Force
                $cert = New-SelfSignedCertificate `
                    -Type Custom `
                    -Subject $Publisher `
                    -KeyUsage DigitalSignature `
                    -FriendlyName "Overmax Dev Certificate" `
                    -CertStoreLocation "Cert:\CurrentUser\My" `
                    -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3") `
                    -NotAfter (Get-Date).AddYears(5)
                
                Export-PfxCertificate -Cert $cert -FilePath $pfxPath -Password $secPass | Out-Null
                Export-Certificate -Cert $cert -FilePath $cerPath -Force | Out-Null
                Write-Host "Exported dev certificate to $pfxPath and $cerPath"

                # Check if running as administrator to register in LocalMachine
                $isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
                if ($isAdmin) {
                    $store = New-Object System.Security.Cryptography.X509Certificates.X509Store("TrustedPeople", "LocalMachine")
                    $store.Open("ReadWrite")
                    $store.Add($cert)
                    $store.Close()
                    Write-Host "Registered dev certificate to LocalMachine\TrustedPeople store."
                } else {
                    $store = New-Object System.Security.Cryptography.X509Certificates.X509Store("TrustedPeople", "CurrentUser")
                    $store.Open("ReadWrite")
                    $store.Add($cert)
                    $store.Close()
                    Write-Host "Registered dev certificate to CurrentUser\TrustedPeople store."
                }
            } elseif (-not (Test-Path $cerPath)) {
                $cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -like "*$Publisher*" } | Select-Object -First 1
                if ($cert) {
                    Export-Certificate -Cert $cert -FilePath $cerPath -Force | Out-Null
                }
            }
        }

        Write-Host "Signing MSIX with signtool..."
        & $signTool sign /fd SHA256 /a /f $pfxPath /p $CertPassword $msixOutput
        if ($LASTEXITCODE -ne 0) {
            throw "SignTool failed with exit code $LASTEXITCODE"
        }
        Write-Host "MSIX package signed successfully."
    }

    # 7. Local Sideloading Installation (Optional)
    if ($Install) {
        Write-Host "Installing (sideloading) $msixOutput..."
        # First remove previous version if installed
        $existing = Get-AppxPackage -Name $PackageName -ErrorAction SilentlyContinue
        if ($existing) {
            Write-Host "Removing existing version $($existing.PackageFullName)..."
            Remove-AppxPackage -Package $existing.PackageFullName
        }

        try {
            Add-AppxPackage -Path $msixOutput -ErrorAction Stop
            Write-Host "Successfully sideloaded $PackageName!"
            
            $installed = Get-AppxPackage -Name $PackageName
            if ($installed) {
                Write-Host "Installed Details:"
                Write-Host "  PackageFullName: $($installed.PackageFullName)"
                Write-Host "  InstallLocation: $($installed.InstallLocation)"
            }
        } catch {
            $err = $_
            Write-Warning "Failed to install MSIX package: $err"
            Write-Host ""
            Write-Host "============================================================" -ForegroundColor Yellow
            Write-Host " [Sideloading Note: Untrusted Root Certificate (0x800B0109)]" -ForegroundColor Yellow
            Write-Host " Windows requires self-signed packages to be trusted in" -ForegroundColor Yellow
            Write-Host " 'LocalMachine\TrustedPeople' before sideloading." -ForegroundColor Yellow
            Write-Host ""
            Write-Host " To trust this dev certificate on your system:" -ForegroundColor Yellow
            Write-Host " 1. Run PowerShell as Administrator, then execute:" -ForegroundColor Cyan
            Write-Host "    Import-Certificate -FilePath '$pfxDir\OvermaxDev.cer' -CertStoreLocation Cert:\LocalMachine\TrustedPeople" -ForegroundColor Cyan
            Write-Host " 2. Re-run:" -ForegroundColor Cyan
            Write-Host "    .\scripts\package-msix.ps1 -SkipBuild -Sign -Install" -ForegroundColor Cyan
            Write-Host "============================================================" -ForegroundColor Yellow
            Write-Host ""
            throw $err
        }
    }

    Write-Host "`n============================================================"
    Write-Host " MSIX Packaging Complete!"
    Write-Host " Package: $msixOutput"
    Write-Host " Version: $msixVersion"
    Write-Host "============================================================"

} finally {
    Pop-Location
}