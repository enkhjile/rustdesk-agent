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

# Exercise the packaged viewer without running setup or requesting elevation.
Add-Type -AssemblyName System.Windows.Forms
$factory = $assembly.GetType('Setup').GetMethod('CreateLicenseWindow', [Reflection.BindingFlags]'Static,NonPublic')
if (!$factory) { throw 'Setup lacks a testable license viewer' }
$window = $factory.Invoke($null, @())
try {
    $window.ShowInTaskbar = $false
    $window.Opacity = 0
    $window.Show()
    $tabs = $window.Controls[0]
    if ($tabs.TabPages.Count -ne 3) { throw 'Missing license notice tabs' }
    foreach ($index in 0,1) {
        $box = $tabs.TabPages[$index].Controls[0]
        if (!$box.ReadOnly -or !$box.Text.Trim()) { throw 'Plain-text notices are not readable' }
    }
    $tabs.SelectedIndex = 2
    $browser = $tabs.TabPages[2].Controls[0]
    if ($browser -isnot [Windows.Forms.WebBrowser]) { throw 'HTML notices must be rendered, not shown as source' }
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        [Windows.Forms.Application]::DoEvents()
        Start-Sleep -Milliseconds 50
    } until (($browser.Document -and $browser.Document.Body -and $browser.ReadyState -eq 'Complete') -or [DateTime]::UtcNow -gt $deadline)
    $text = $browser.Document.Body.InnerText
    if ($text -notmatch 'Copyright notices for The Rust Standard Library' -or $text -notmatch 'Apache' -or $text -match '<!DOCTYPE|<html|<h1>') {
        throw 'Rust license HTML was not rendered as readable text'
    }
    $browser.Navigate('https://example.invalid/')
    [Windows.Forms.Application]::DoEvents()
    if ($browser.Url.GetLeftPart([UriPartial]::Path) -ne 'about:blank') { throw 'License viewer allowed external navigation' }
} finally { $window.Dispose() }
'License viewer renders bundled HTML, retains text notices and blocks external navigation'
