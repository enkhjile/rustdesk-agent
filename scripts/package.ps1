#Requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$BinaryPath,
    [Parameter(Mandatory)][string]$NoticesPath,
    [Parameter(Mandatory)][string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$binary = (Resolve-Path -LiteralPath $BinaryPath).Path
$notices = (Resolve-Path -LiteralPath $NoticesPath).Path
$output = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $output) { throw 'Package output must be a new directory' }
if ((Get-Item -LiteralPath $notices).Length -eq 0) { throw 'License notices are empty' }
Push-Location (Join-Path $PSScriptRoot '..')
try {
    $source = git rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Cannot identify checked-out source' }
    $compiler = rustc --version
    if ($LASTEXITCODE -ne 0) { throw 'Cannot identify Rust compiler' }
    $sysroot = rustc --print sysroot
    if ($LASTEXITCODE -ne 0) { throw 'Cannot identify Rust sysroot' }
    $package = Join-Path $output 'package'
    $distribution = Join-Path $output 'distribution'
    New-Item -ItemType Directory $package, $distribution | Out-Null
    Copy-Item -LiteralPath $binary -Destination "$package/rustdesk-agent.exe"
    Copy-Item -LiteralPath $notices -Destination "$package/THIRD-PARTY-NOTICES.txt"
    Copy-Item scripts/install.ps1, LICENSE -Destination $package
    Copy-Item docs/ci-test-package.md "$package/README.md"
    Copy-Item "$sysroot/share/doc/rust/COPYRIGHT-library.html" "$package/RUST-STANDARD-LIBRARY-NOTICES.html"
    [ordered]@{
        source_commit = $source
        workflow_run = if ($env:GITHUB_RUN_ID) { "$env:GITHUB_SERVER_URL/$env:GITHUB_REPOSITORY/actions/runs/$env:GITHUB_RUN_ID" } else { $null }
        run_attempt = $env:GITHUB_RUN_ATTEMPT
        target = 'x86_64-pc-windows-msvc'
        profile = 'release'
        features = @()
        rustc = $compiler
        signing = 'unsigned'
    } | ConvertTo-Json | Set-Content "$package/build.json" -Encoding utf8NoBOM
    Get-ChildItem -LiteralPath $package -File | Sort-Object Name | ForEach-Object {
        '{0}  {1}' -f (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant(), $_.Name
    } | Set-Content "$package/SHA256SUMS" -Encoding ascii
    $zip = Join-Path $distribution 'rustdesk-agent-windows-x64-unsigned.zip'
    Compress-Archive -Path "$package/*" -DestinationPath $zip
    '{0}  {1}' -f (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant(), (Split-Path $zip -Leaf) |
        Set-Content "$distribution/SHA256SUMS" -Encoding ascii
} finally {
    Pop-Location
}
