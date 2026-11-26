param(
    [switch]$SkipInstall,
    [switch]$PackageOnly,
    [switch]$Unsigned,
    [switch]$Portable,
    [string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
$SourceRoot = Split-Path -Parent $PSScriptRoot
$Version = (Get-Content -LiteralPath (Join-Path $SourceRoot 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json).version
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path (Join-Path (Split-Path -Parent $SourceRoot) 'Discord Quest Helper Releases') $Version
}
$ReleaseDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$NormalizedSource = [IO.Path]::GetFullPath($SourceRoot)
if ($ReleaseDirectory.Equals($NormalizedSource, [StringComparison]::OrdinalIgnoreCase) -or
    $ReleaseDirectory.StartsWith($NormalizedSource + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'The distribution folder must be outside the source folder.'
}
Set-Location -LiteralPath $SourceRoot

function Invoke-Step([string]$Label, [scriptblock]$Command) {
    Write-Host $Label -ForegroundColor Cyan
    & $Command
    if ($LASTEXITCODE -ne 0) { throw "$Label failed (exit $LASTEXITCODE)" }
}

if (-not $Unsigned -and -not $Portable -and -not $env:TAURI_SIGNING_PRIVATE_KEY) {
    $KeyDirectory = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'DiscordQuestHelper-Signing'
    $KeyFile = Join-Path $KeyDirectory 'update.key'
    $PasswordFile = Join-Path $KeyDirectory 'password.dpapi'
    if (Test-Path -LiteralPath $KeyFile) {
        $env:TAURI_SIGNING_PRIVATE_KEY = $KeyFile
        if (Test-Path -LiteralPath $PasswordFile) {
            $SecurePassword = Get-Content -LiteralPath $PasswordFile -Raw | ConvertTo-SecureString
            $Credential = New-Object System.Net.NetworkCredential('signing', $SecurePassword)
            $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $Credential.Password
        }
    } else {
        throw 'A signing key is required for release setup builds. Use -Unsigned for local/CI validation only.'
    }
}

if (-not $PackageOnly) {
    if (-not $SkipInstall) {
        Invoke-Step 'Install frontend dependencies' { corepack pnpm install --frozen-lockfile }
    }
    Invoke-Step 'Check runner formatting' { cargo fmt --manifest-path src-win/Cargo.toml --check }
    Invoke-Step 'Test runner' { cargo test --manifest-path src-win/Cargo.toml --locked }
    Invoke-Step 'Lint runner' { cargo clippy --manifest-path src-win/Cargo.toml --all-targets --locked -- -D warnings }
    Invoke-Step 'Test app and release tools' { corepack pnpm test }
    Invoke-Step 'Build embedded runner' { corepack pnpm run sync:runner }
    Invoke-Step 'Check backend formatting' { cargo fmt --manifest-path src-tauri/Cargo.toml --check }
    Invoke-Step 'Test backend' { cargo test --manifest-path src-tauri/Cargo.toml --locked }
    Invoke-Step 'Lint backend' { cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings }
    if ($Portable) {
        Invoke-Step 'Build portable application' { corepack pnpm tauri build --no-bundle }
    } elseif ($Unsigned) {
        $ValidationConfig = Join-Path ([IO.Path]::GetTempPath()) ("dqh-validation-" + [guid]::NewGuid().ToString('N') + '.json')
        [IO.File]::WriteAllText($ValidationConfig, '{"bundle":{"createUpdaterArtifacts":false}}')
        try {
            Invoke-Step 'Build validation setup' { corepack pnpm tauri build --bundles nsis --config $ValidationConfig }
        } finally {
            Remove-Item -LiteralPath $ValidationConfig -ErrorAction SilentlyContinue
        }
    } else {
        Invoke-Step 'Build signed setup' { corepack pnpm tauri build --bundles nsis }
    }
}

New-Item -ItemType Directory -Force -Path $ReleaseDirectory | Out-Null
if ($Portable) {
    $Executable = Join-Path $SourceRoot 'src-tauri/target/release/discord-quest-helper.exe'
    $Destination = Join-Path $ReleaseDirectory 'DiscordQuestHelper.exe'
    Copy-Item -LiteralPath $Executable -Destination $Destination -Force
} else {
    $Installers = @(Get-ChildItem -LiteralPath (Join-Path $SourceRoot 'src-tauri/target/release/bundle/nsis') -Filter "*_${Version}_x64-setup.exe")
    if ($Installers.Count -ne 1) { throw "Expected one setup for version $Version, found $($Installers.Count)" }
    $Destination = Join-Path $ReleaseDirectory "DiscordQuestHelper-$Version-windows-x64-setup.exe"
    Copy-Item -LiteralPath $Installers[0].FullName -Destination $Destination -Force
    if (-not $Unsigned) {
        $Signature = $Installers[0].FullName + '.sig'
        if (-not (Test-Path -LiteralPath $Signature)) { throw 'Installer signature was not generated' }
        Copy-Item -LiteralPath $Signature -Destination ($Destination + '.sig') -Force
        Invoke-Step 'Verify setup signature and reject modified bytes' {
            cargo run --manifest-path src-tauri/Cargo.toml --locked --example verify_update -- $Destination ($Destination + '.sig')
        }
    }
}
$Hash = (Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -LiteralPath (Join-Path $ReleaseDirectory 'SHA256SUMS.txt') -Value "$Hash  $(Split-Path -Leaf $Destination)" -Encoding ascii
Write-Host "Application package: $Destination" -ForegroundColor Green
