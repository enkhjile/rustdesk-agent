#Requires -Version 5.1
#Requires -RunAsAdministrator
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][ValidateSet('Install','Upgrade','Uninstall')][string]$Action,
    [string]$SourcePath,
    [string]$Sha256,
    [string]$RustDeskPath = (Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'RustDesk\rustdesk.exe')
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($PSVersionTable.PSEdition -ne 'Desktop' -or ![Environment]::Is64BitProcess -or $env:PROCESSOR_ARCHITECTURE -ne 'AMD64') {
    throw 'Use elevated Windows PowerShell 5.1 x64'
}
$programFiles = [Environment]::GetFolderPath('ProgramFiles')
$programData = [Environment]::GetFolderPath('CommonApplicationData')
$directory = Join-Path $programFiles 'RustDeskAgent'
$reports = Join-Path $programData 'RustDeskAgent'
$executable = Join-Path $directory 'rustdesk-agent.exe'
$stage = Join-Path $directory 'rustdesk-agent.new'
$backup = Join-Path $directory 'rustdesk-agent.previous'
$sc = Join-Path ([Environment]::GetFolderPath('System')) 'sc.exe'

function Assert-PlainPath([string]$path) {
    if ($path -notmatch '^[A-Za-z]:\\' -or $path -match '["\x00-\x1f]' -or $path.Substring(2).Contains(':')) {
        throw 'Expected an absolute local path without quotes, streams or control characters'
    }
    $part = [IO.Path]::GetFullPath($path)
    while ($part) {
        if ((Test-Path -LiteralPath $part) -and ((Get-Item -LiteralPath $part -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "Reparse points are not supported: $part"
        }
        $part = [IO.Path]::GetDirectoryName($part)
    }
}
function Assert-TrustedPath([string]$path, [bool]$parentOnly = $false) {
    Assert-PlainPath $path
    $acl = Get-Acl -LiteralPath $path
    # TrustedInstaller owns standard Windows directories. Ordinary accounts may read,
    # but may not replace executables or change their permissions.
    $trusted = @('S-1-5-18','S-1-5-32-544','S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464')
    if ($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -notin $trusted) { throw "Untrusted owner: $path" }
    # ProgramData normally lets users create their own children. It must not let
    # them delete our protected child or take ownership/change the parent DACL.
    $writeMask = if ($parentOnly) { 0x000d0040 } else { 0x000d0156 }
    foreach ($rule in $acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])) {
        if ($rule.AccessControlType -eq 'Allow' -and !($rule.PropagationFlags -band [Security.AccessControl.PropagationFlags]::InheritOnly) -and
            $rule.IdentityReference.Value -notin $trusted -and ([int]$rule.FileSystemRights -band $writeMask)) {
            throw "Untrusted write access: $path"
        }
    }
}
function Directory-Policy([bool]$writable) {
    $rights = if ($writable) {'0x1301bf'} else {'0x1200a9'}
    $ownerRights = if ($writable) {'(A;OICI;RC;;;OW)'} else {''}
    $acl = New-Object Security.AccessControl.DirectorySecurity
    $acl.SetSecurityDescriptorSddlForm("O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;$rights;;;$serviceSid)$ownerRights")
    return $acl
}
function Assert-Directory([string]$path, [bool]$writable) {
    Assert-PlainPath $path
    if (!(Test-Path -LiteralPath $path -PathType Container)) { throw "Expected directory: $path" }
    $actual = Get-Acl -LiteralPath $path
    $expected = Directory-Policy $writable
    # Set-Acl can add auto-inheritance bookkeeping flags to a protected DACL.
    # Compare its ACEs exactly; check inheritance protection separately below.
    $actualRules = $actual.GetSecurityDescriptorSddlForm('Access') -replace '^D:[^(]*','D:'
    $expectedRules = $expected.GetSecurityDescriptorSddlForm('Access') -replace '^D:[^(]*','D:'
    if (!$actual.AreAccessRulesProtected -or $actual.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne 'S-1-5-32-544' -or
        $actualRules -ne $expectedRules) {
        throw "Unexpected directory permissions; refusing to repair or adopt: $path (actual: $($actual.Sddl); expected: $($expected.GetSecurityDescriptorSddlForm('All')))"
    }
}
function Stop-Agent {
    $controller = New-Object ServiceProcess.ServiceController('RustDeskAgent')
    try {
        if ($controller.Status -ne 'Stopped') { $controller.Stop(); $controller.WaitForStatus('Stopped',[TimeSpan]::FromSeconds(30)) }
    } finally { $controller.Dispose() }
}

# Serialize setup independently of the service's durable-state writer lock.
$security = New-Object Security.AccessControl.MutexSecurity
$security.SetSecurityDescriptorSddlForm('O:BAG:BAD:P(A;;GA;;;SY)(A;;GA;;;BA)')
$created = $false
$mutex = [Threading.Mutex]::new($false,'Global\RustDeskAgentInstaller',[ref]$created,$security)
try {
    if (!$created) { throw 'Another installer is running; retry after it exits' }
    Assert-TrustedPath $programFiles
    Assert-TrustedPath $programData $true
    Assert-PlainPath $directory
    Assert-PlainPath $reports
    $sidOutput = & $sc showsid RustDeskAgent
    if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve service SID' }
    $serviceSid = [regex]::Match(($sidOutput -join ' '),'S-1-5-80-(\d+-){4}\d+').Value
    if (!$serviceSid) { throw 'Missing service SID' }
    Assert-PlainPath $RustDeskPath
    $RustDeskPath = [IO.Path]::GetFullPath($RustDeskPath)
    $command = '"{0}" --service "{1}" "{2}"' -f $executable,$RustDeskPath,(Join-Path $reports 'detection.json')
    $service = Get-CimInstance Win32_Service -Filter "Name='RustDeskAgent'"
    if ($Action -eq 'Install') {
        if ($service -or (Test-Path -LiteralPath $directory)) { throw 'Installation already exists; use Upgrade or investigate partial setup' }
    } else {
        if (!$service -or $service.PathName -cne $command -or $service.StartName -ne 'NT AUTHORITY\LocalService' -or
            (Get-ItemProperty HKLM:\SYSTEM\CurrentControlSet\Services\RustDeskAgent).ServiceSidType -ne 1) {
            throw 'Service is not managed by this installer (path/account/SID mismatch)'
        }
        Assert-Directory $directory $false
        Assert-TrustedPath $executable
        if (@(Get-ChildItem -LiteralPath $directory -Force | Where-Object Name -ne 'rustdesk-agent.exe').Count) {
            throw 'Unexpected files or interrupted upgrade; inspect installation before continuing'
        }
        if ($Action -eq 'Upgrade' -and $service.State -notin @('Running','Stopped')) {
            throw 'Wait for the service transition before upgrading'
        }
    }
    if (Test-Path -LiteralPath $reports) { Assert-Directory $reports $true }
    Assert-PlainPath (Join-Path $reports 'detection.json')
    if ($Action -eq 'Uninstall') {
        Stop-Agent
        $result = Invoke-CimMethod -InputObject $service -MethodName Delete
        if ($result.ReturnValue -ne 0) { throw "Service deletion failed: $($result.ReturnValue)" }
        # Fixed, validated paths only; never recurse into retained device data.
        [IO.File]::Delete($executable)
        [IO.Directory]::Delete($directory,$false)
        Write-Output 'Service and executable removed. Enrollment, state and reports retained.'
        return
    }
    if (!$SourcePath -or $Sha256 -notmatch '^[0-9a-fA-F]{64}$') { throw 'SourcePath and trusted SHA-256 are required' }
    Assert-PlainPath $SourcePath
    if (!(Test-Path -LiteralPath $SourcePath -PathType Leaf) -or (Get-FileHash -LiteralPath $SourcePath -Algorithm SHA256).Hash -ne $Sha256) {
        throw 'Source executable SHA-256 mismatch'
    }
    if (!$RustDeskPath.StartsWith($programFiles + '\',[StringComparison]::OrdinalIgnoreCase) -or !(Test-Path -LiteralPath $RustDeskPath -PathType Leaf)) {
        throw 'RustDesk must be installed under Program Files'
    }
    $part = $RustDeskPath
    while ($part -ne $programFiles) { Assert-TrustedPath $part; $part = [IO.Path]::GetDirectoryName($part) }
    # Reject non-x64 payloads before stopping any existing service. Hash verification
    # is integrity checking, not publisher authentication or an application verifier.
    $stream = [IO.File]::OpenRead($SourcePath)
    try {
        $reader = New-Object IO.BinaryReader($stream)
        if ($reader.ReadUInt16() -ne 0x5a4d) { throw 'Expected Windows PE executable' }
        $stream.Position = 0x3c; $offset = $reader.ReadUInt32()
        if ($offset -gt $stream.Length - 6) { throw 'Invalid PE header' }
        $stream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664) { throw 'Expected x64 PE executable' }
    } finally { $stream.Dispose() }
    if ($Action -eq 'Install') {
        [IO.DirectoryInfo]::new($directory).Create((Directory-Policy $false))
        if (!(Test-Path -LiteralPath $reports)) { [IO.DirectoryInfo]::new($reports).Create((Directory-Policy $true)) }
    }
    Assert-Directory $directory $false
    Assert-Directory $reports $true
    [IO.File]::Copy($SourcePath,$stage,$false)
    if ((Get-FileHash -LiteralPath $stage -Algorithm SHA256).Hash -ne $Sha256) {
        [IO.File]::Delete($stage); throw 'Staged executable SHA-256 mismatch'
    }
    if ($Action -eq 'Install') {
        [IO.File]::Move($stage,$executable)
        $result = Invoke-CimMethod -ClassName Win32_Service -MethodName Create -Arguments @{
            Name='RustDeskAgent';DisplayName='RustDesk Agent';PathName=$command;
            ServiceType=[byte]16;ErrorControl=[byte]1;StartMode='Automatic';StartName='NT AUTHORITY\LocalService'
        }
        if ($result.ReturnValue -ne 0) { throw "Service creation failed: $($result.ReturnValue); files retained for inspection" }
        & $sc sidtype RustDeskAgent unrestricted | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Service SID configuration failed; leave service stopped' }
        Write-Output 'Installed, service stopped. Enroll and initialize state as administrator, bind UUID in Console, then Start-Service RustDeskAgent.'
    } else {
        $wasRunning = $service.State -eq 'Running'
        Stop-Agent
        [IO.File]::Replace($stage,$executable,$backup)
        if ($wasRunning) {
            Start-Service RustDeskAgent
            Start-Sleep -Seconds 2
            if ((Get-Service RustDeskAgent).Status -ne 'Running') { throw 'Upgraded service stopped; previous executable retained for administrator recovery' }
        }
        [IO.File]::Delete($backup)
        Write-Output 'Upgraded. Enrollment and state untouched; previous service running/stopped status restored.'
    }
} finally { $mutex.Dispose() }
