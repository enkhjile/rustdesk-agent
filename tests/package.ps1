#Requires -Version 7.0
param(
    [Parameter(Mandatory)][string]$ZipPath,
    [Parameter(Mandatory)][string]$ExpectedCommit,
    [Parameter(Mandatory)][string]$ExpectedBinarySha256
)
$ErrorActionPreference = 'Stop'
$zip = [IO.Compression.ZipFile]::OpenRead((Resolve-Path -LiteralPath $ZipPath).Path)
try {
    $expected = @('rustdesk-agent.exe', 'install.ps1', 'LICENSE', 'README.md',
        'THIRD-PARTY-NOTICES.txt', 'RUST-STANDARD-LIBRARY-NOTICES.html', 'build.json', 'SHA256SUMS')
    if (@(Compare-Object ($expected | Sort-Object) ($zip.Entries.FullName | Sort-Object)).Count) {
        throw 'Package entries differ from the distribution contract'
    }
    function Read-Entry($name) {
        $reader = [IO.StreamReader]::new($zip.GetEntry($name).Open())
        try { $reader.ReadToEnd() } finally { $reader.Dispose() }
    }
    $manifest = Read-Entry 'build.json' | ConvertFrom-Json
    if ($manifest.source_commit -cne $ExpectedCommit -or $manifest.signing -cne 'unsigned' -or
        $manifest.target -cne 'x86_64-pc-windows-msvc' -or $manifest.profile -cne 'release') {
        throw 'Incorrect package provenance'
    }
    $seen = @{}
    foreach ($line in ((Read-Entry 'SHA256SUMS') -split '\r?\n' | Where-Object { $_ })) {
        if ($line -cnotmatch '^([a-f0-9]{64})  ([A-Za-z0-9._-]+)$') { throw 'Invalid checksum line' }
        $hash = $Matches[1]; $name = $Matches[2]
        if ($seen.ContainsKey($name) -or $name -eq 'SHA256SUMS') { throw 'Duplicate or self checksum' }
        $entry = $zip.GetEntry($name)
        if (!$entry -or $entry.Length -eq 0) { throw 'Missing or empty package file' }
        $stream = $entry.Open()
        try { $actual = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)) }
        finally { $stream.Dispose() }
        if ($actual -ine $hash) { throw "Checksum mismatch: $name" }
        if ($name -eq 'rustdesk-agent.exe' -and $actual -ine $ExpectedBinarySha256) {
            throw 'Packaged executable differs from the checked binary'
        }
        $seen[$name] = $true
    }
    if ($seen.Count -ne $expected.Count - 1) { throw 'Incomplete checksum manifest' }
    if (!(Read-Entry 'LICENSE').Contains('MIT License') -or
        !(Read-Entry 'THIRD-PARTY-NOTICES.txt').Contains('ring ') -or
        !(Read-Entry 'README.md').Contains('unsigned')) { throw 'Required license or unsigned notice missing' }
    'Package integrity, provenance and notices passed'
} finally { $zip.Dispose() }
