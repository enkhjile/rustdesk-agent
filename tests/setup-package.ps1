#Requires -Version 5.1
param([Parameter(Mandatory)][string]$SetupPath, [Parameter(Mandatory)][string]$AgentPath)
$ErrorActionPreference = 'Stop'
$assembly = [Reflection.Assembly]::LoadFile((Resolve-Path $SetupPath).Path)
foreach ($name in @('rustdesk-agent.exe','install.ps1','LICENSE','THIRD-PARTY-NOTICES.txt','RUST-STANDARD-LIBRARY-NOTICES.html')) {
    $stream = $assembly.GetManifestResourceStream($name)
    if (!$stream -or !$stream.Length) { throw "Missing setup resource: $name" }
    try {
        if ($name -eq 'rustdesk-agent.exe') {
            $sha = [Security.Cryptography.SHA256]::Create()
            try { $actual = [BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-','') }
            finally { $sha.Dispose() }
            if ($actual -ne (Get-FileHash $AgentPath).Hash) { throw 'Embedded agent differs from checked build' }
        }
    } finally { $stream.Dispose() }
}
$bytes = [IO.File]::ReadAllBytes((Resolve-Path $SetupPath).Path)
if (![Text.Encoding]::UTF8.GetString($bytes).Contains('level="requireAdministrator"')) { throw 'Setup must request elevation once' }
'Setup resources, agent hash and elevation manifest passed'
$plan = $assembly.GetType('Setup').GetMethod('NeedsCompletion', [Reflection.BindingFlags]'Static,NonPublic')
foreach ($case in @(
    @($false,$false,$false,0,$true), # fresh
    @($true,$true,$true,0,$false), # normal upgrade, preserve stopped/running status
    @($false,$true,$true,0,$true), # retained-data reinstall must start
    @($true,$false,$false,1,$true), # interrupted first enrollment
    @($true,$true,$false,1,$true), # interrupted first initialization
    @($true,$true,$true,2,$true) # interrupted first startup
)) {
    if ($plan.Invoke($null,[object[]]$case[0..3]) -ne $case[4]) { throw 'Incorrect setup recovery decision' }
}
foreach ($case in @(@($true,$false,$false,0),@($true,$true,$false,0),@($true,$false,$true,0),@($false,$false,$true,0),@($true,$true,$false,2))) {
    $rejected = $false
    try { $plan.Invoke($null,[object[]]$case) | Out-Null } catch {
        $errorCause = $_.Exception
        while ($errorCause.InnerException) { $errorCause = $errorCause.InnerException }
        if ($errorCause -isnot [InvalidOperationException]) { throw }
        $rejected = $true
    }
    if (!$rejected) { throw 'Missing established enrollment/state was accepted' }
}
'Fresh setup, upgrade, reinstall and fail-closed recovery decisions passed'
