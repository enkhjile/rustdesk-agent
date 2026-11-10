#Requires -Version 5.1
#Requires -RunAsAdministrator
param(
    [Parameter(Mandatory=$true)][string]$AgentPath,
    [Parameter(Mandatory=$true)][string]$RustDeskPath
)
# Destructive only to this test's fresh installation. Refuse an existing service.
$ErrorActionPreference = 'Stop'
$installer = Join-Path $PSScriptRoot '..\scripts\install.ps1'
$directory = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'RustDeskAgent'
$reports = Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'RustDeskAgent'
if ((Get-Service RustDeskAgent -ErrorAction SilentlyContinue) -or (Test-Path $directory) -or (Test-Path $reports)) {
    throw 'Installer tests require a disposable machine with no existing installation/report directory'
}
function Assert($condition, $message) { if (!$condition) { throw $message } }
function Reject([scriptblock]$operation, $message, $pattern) {
    $rejected = $false
    try { & $operation } catch { if ($_.Exception.Message -notmatch $pattern) { throw }; $rejected = $true }
    Assert $rejected $message
}
$hash = (Get-FileHash -LiteralPath $AgentPath -Algorithm SHA256).Hash
Reject { & $installer -Action Install -SourcePath $AgentPath -Sha256 ('0' * 64) -RustDeskPath $RustDeskPath } 'Bad hash accepted' 'SHA-256 mismatch'
Assert (!(Test-Path $directory)) 'Failed validation changed installation'
$invalid = Join-Path $env:TEMP ('invalid-agent-' + [guid]::NewGuid() + '.exe')
try {
    [IO.File]::WriteAllBytes($invalid,[byte[]](0,0,0,0))
    $invalidHash = (Get-FileHash $invalid).Hash
    Reject { & $installer -Action Install -SourcePath $invalid -Sha256 $invalidHash -RustDeskPath $RustDeskPath } 'Non-PE payload accepted' 'Expected Windows PE'
    Assert (!(Test-Path $directory)) 'Rejected payload changed installation'
} finally { Remove-Item -LiteralPath $invalid -ErrorAction SilentlyContinue }
& $installer -Action Install -SourcePath $AgentPath -Sha256 $hash -RustDeskPath $RustDeskPath
$service = Get-CimInstance Win32_Service -Filter "Name='RustDeskAgent'"
Assert ($service.StartName -eq 'NT AUTHORITY\LocalService') 'Wrong service account'
Assert ($service.State -eq 'Stopped') 'Install unexpectedly started service'
Assert ($service.StartMode -eq 'Auto') 'Service is not automatic'
Assert ($service.PathName -notmatch 'lab-apply') 'Writes enabled'
Assert ((Get-ItemProperty HKLM:\SYSTEM\CurrentControlSet\Services\RustDeskAgent).ServiceSidType -eq 1) 'Missing service SID'
Assert ((Get-FileHash "$directory\rustdesk-agent.exe").Hash -eq $hash) 'Installed bytes differ'
Assert (Get-Acl $directory).AreAccessRulesProtected 'Executable directory inherits permissions'
Assert (Get-Acl $reports).AreAccessRulesProtected 'Report directory inherits permissions'
Reject { & $installer -Action Install -SourcePath $AgentPath -Sha256 $hash -RustDeskPath $RustDeskPath } 'Duplicate installation accepted' 'already exists'
Reject { & $installer -Action Upgrade -SourcePath $AgentPath -Sha256 ('0' * 64) -RustDeskPath $RustDeskPath } 'Bad upgrade hash accepted' 'SHA-256 mismatch'
Assert ((Get-FileHash "$directory\rustdesk-agent.exe").Hash -eq $hash) 'Rejected upgrade changed executable'
$savedAcl = Get-Acl $directory
$unsafeAcl = Get-Acl $directory
$unsafeAcl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-5-32-545'),'Modify','ContainerInherit,ObjectInherit','None','Allow'))
Set-Acl -LiteralPath $directory -AclObject $unsafeAcl
try { Reject { & $installer -Action Upgrade -SourcePath $AgentPath -Sha256 $hash -RustDeskPath $RustDeskPath } 'Unsafe installation permissions accepted' 'Unexpected directory permissions' }
finally { Set-Acl -LiteralPath $directory -AclObject $savedAcl }
Set-Content -LiteralPath "$directory\unexpected.txt" -Value 'test marker'
try { Reject { & $installer -Action Uninstall -RustDeskPath $RustDeskPath } 'Uninstall accepted unexpected files' 'Unexpected files' }
finally { Remove-Item -LiteralPath "$directory\unexpected.txt" }
& $installer -Action Upgrade -SourcePath $AgentPath -Sha256 $hash -RustDeskPath $RustDeskPath
Assert ((Get-Service RustDeskAgent).Status -eq 'Stopped') 'Upgrade started a stopped service'
& $installer -Action Uninstall -RustDeskPath $RustDeskPath
Assert (!(Get-Service RustDeskAgent -ErrorAction SilentlyContinue)) 'Uninstall left service'
Assert (!(Test-Path $directory)) 'Uninstall left executable directory'
Assert (Test-Path $reports) 'Uninstall removed retained reports'
& $installer -Action Install -SourcePath $AgentPath -Sha256 $hash -RustDeskPath $RustDeskPath
& $installer -Action Uninstall -RustDeskPath $RustDeskPath
'Installer lifecycle checks passed'
