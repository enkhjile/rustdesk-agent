#Requires -Version 5.1
param([Parameter(Mandatory)][string]$BinaryPath, [Parameter(Mandatory)][string]$OutputPath,
    [Parameter(Mandatory)][string]$NoticesPath)
$ErrorActionPreference = 'Stop'
$compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
$repo = Split-Path $PSScriptRoot
$sysroot = rustc --print sysroot
if ($LASTEXITCODE -ne 0) { throw 'Cannot locate Rust license notices' }
& $compiler /nologo /target:winexe /platform:x64 /optimize+ "/out:$OutputPath" `
    "/win32manifest:$repo\installer\setup.manifest" `
    /reference:System.Windows.Forms.dll /reference:System.Drawing.dll /reference:System.ServiceProcess.dll `
    "/resource:$BinaryPath,rustdesk-agent.exe" "/resource:$repo\scripts\install.ps1,install.ps1" `
    "/resource:$repo\LICENSE,LICENSE" "/resource:$NoticesPath,THIRD-PARTY-NOTICES.txt" `
    "/resource:$sysroot\share\doc\rust\COPYRIGHT-library.html,RUST-STANDARD-LIBRARY-NOTICES.html" `
    "$repo\installer\Setup.cs"
if ($LASTEXITCODE -ne 0) { throw 'Setup compilation failed' }
