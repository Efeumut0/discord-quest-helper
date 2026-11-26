param(
    [switch]$SkipInstall,
    [switch]$PackageOnly,
    [string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
$SourceRoot = Split-Path -Parent $PSScriptRoot
if (-not $OutputDirectory) {
    $SourceName = Split-Path -Leaf $SourceRoot
    $DistributionName = if ($SourceName -like 'discord-quest-helper-source-win32-x64*') {
        $SourceName.Replace('-source-win32-x64', '-win32-x64')
    } else {
        'discord-quest-helper-win32-x64'
    }
    $OutputDirectory = Join-Path (Split-Path -Parent $SourceRoot) $DistributionName
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

if (-not $PackageOnly) {
    if (-not $SkipInstall) {
        Invoke-Step 'Install frontend dependencies' { corepack pnpm install --frozen-lockfile }
    }
    Invoke-Step 'Check runner formatting' { cargo fmt --manifest-path src-win/Cargo.toml --check }
    Invoke-Step 'Test runner' { cargo test --manifest-path src-win/Cargo.toml --locked }
    Invoke-Step 'Lint runner' { cargo clippy --manifest-path src-win/Cargo.toml --all-targets --locked -- -D warnings }
    Invoke-Step 'Test source selection' { corepack pnpm test }
    Invoke-Step 'Build embedded runner' { corepack pnpm run sync:runner }
    Invoke-Step 'Check backend formatting' { cargo fmt --manifest-path src-tauri/Cargo.toml --check }
    Invoke-Step 'Test backend' { cargo test --manifest-path src-tauri/Cargo.toml --locked }
    Invoke-Step 'Lint backend' { cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings }
    Invoke-Step 'Build portable application' { corepack pnpm tauri build --no-bundle }
}

New-Item -ItemType Directory -Force -Path $ReleaseDirectory | Out-Null
$Executable = Join-Path $SourceRoot 'src-tauri/target/release/discord-quest-helper.exe'
if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) { throw 'Release executable was not generated' }
$Destination = Join-Path $ReleaseDirectory 'DiscordQuestHelper.exe'
Copy-Item -LiteralPath $Executable -Destination $Destination -Force
$Hash = (Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -LiteralPath (Join-Path $ReleaseDirectory 'SHA256SUMS.txt') -Value "$Hash  DiscordQuestHelper.exe" -Encoding ascii
Write-Host "Portable application: $Destination" -ForegroundColor Green
