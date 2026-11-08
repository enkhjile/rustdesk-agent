param([Parameter(Mandatory=$true)][PSCredential]$Credential)
$ErrorActionPreference = 'Stop'
# Deliberately restricted to the disposable lab; never install on a real client.
if ($env:COMPUTERNAME -ne 'RDAGENT-LAB' -or $Credential.UserName -ne 'RDAGENT-LAB\LabUser') {
    throw 'This harness requires RDAGENT-LAB and its dummy LabUser account'
}
$name = 'RustDeskLabUserInit'
$output = 'C:\Lab\Results\user-init-service.txt'
if (Get-Service $name -ErrorAction SilentlyContinue) { throw 'Probe service already exists' }
if (Test-Path $output) { throw 'Archive previous probe output before rerunning' }
Stop-Service RustDeskAgent -ErrorAction Stop
Stop-ScheduledTask RustDeskLabEndpoint
$state = 'C:\ProgramData\RustDeskAgentState\state.json'
$before = (Get-FileHash $state).Hash
$acl = (Get-Acl $state).Sddl
@'
using System;
using System.Diagnostics;
using System.IO;
using System.Security.Principal;
using System.ServiceProcess;
using System.Threading;
class UserInitProbe : ServiceBase {
    protected override void OnStart(string[] args) {
        ThreadPool.QueueUserWorkItem(delegate {
            using (var log = new StreamWriter(@"C:\Lab\Results\user-init-service.txt")) {
                try {
                    var identity = WindowsIdentity.GetCurrent();
                    log.WriteLine("account=" + identity.Name);
                    log.WriteLine("sid=" + identity.User.Value);
                    foreach (var group in identity.Groups) log.WriteLine("group=" + group.Value);
                    bool admin = new WindowsPrincipal(identity).IsInRole(WindowsBuiltInRole.Administrator);
                    log.WriteLine("administrator=" + admin);
                    if (admin) throw new Exception("Probe unexpectedly elevated");
                    var info = new ProcessStartInfo(@"C:\Lab\rustdesk-agent.exe", "--init-state") {
                        UseShellExecute = false, CreateNoWindow = true,
                        RedirectStandardOutput = true, RedirectStandardError = true,
                        WorkingDirectory = @"C:\Lab"
                    };
                    using (var child = Process.Start(info)) {
                        var stdout = child.StandardOutput.ReadToEndAsync();
                        var stderr = child.StandardError.ReadToEndAsync();
                        if (!child.WaitForExit(15000)) { child.Kill(); throw new Exception("CLI timeout"); }
                        log.WriteLine("exit=" + child.ExitCode);
                        log.WriteLine("stdout=" + stdout.Result.Trim());
                        log.WriteLine("stderr=" + stderr.Result.Trim());
                    }
                    log.WriteLine("complete");
                } catch (Exception error) { log.WriteLine("harness-error=" + error); }
            }
        });
    }
    static void Main() { Run(new UserInitProbe { ServiceName = "RustDeskLabUserInit" }); }
}
'@ | Set-Content C:\Lab\UserInitProbe.cs -Encoding ASCII
& C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe /nologo /target:exe /out:C:\Lab\UserInitProbe.exe /reference:System.ServiceProcess.dll C:\Lab\UserInitProbe.cs
if ($LASTEXITCODE -ne 0) { throw 'Probe compilation failed' }
# Preserve the original service-logon assignment; change only this one right.
secedit.exe /export /cfg C:\Lab\user-init-rights.inf /areas USER_RIGHTS /quiet
if ($LASTEXITCODE -ne 0) { throw 'Cannot export user rights' }
$line = Get-Content C:\Lab\user-init-rights.inf | Where-Object { $_ -match '^SeServiceLogonRight\s*=' }
$original = if ($line) { ($line -split '=',2)[1].Trim() } else { '' }
function Set-ServiceLogonRight([string]$value) {
    @('[Unicode]','Unicode=yes','[Version]','signature="$CHICAGO$"','Revision=1','[Privilege Rights]',"SeServiceLogonRight = $value") |
        Set-Content C:\Lab\user-init-right.inf -Encoding Unicode
    secedit.exe /configure /db C:\Lab\user-init-right.sdb /cfg C:\Lab\user-init-right.inf /areas USER_RIGHTS /quiet
    if ($LASTEXITCODE -ne 0) {
        "secedit exit=$LASTEXITCODE"
        Get-Content C:\Lab\user-init-right.inf
        Get-Content C:\Windows\security\logs\scesrv.log -Tail 40 -ErrorAction SilentlyContinue
        throw 'Cannot apply service-logon right'
    }
}
$sid = (Get-LocalUser LabUser).SID.Value
try {
    Set-ServiceLogonRight ((@($original -split ',' | Where-Object { $_ }) + "*$sid" | Select-Object -Unique) -join ',')
    New-Service -Name $name -BinaryPathName 'C:\Lab\UserInitProbe.exe' -Credential $Credential -StartupType Manual | Out-Null
    Start-Service $name
    $deadline = (Get-Date).AddSeconds(30)
    do {
        Start-Sleep -Milliseconds 300
        $lines = @(Get-Content $output -ErrorAction SilentlyContinue)
    } until ($lines -contains 'complete' -or ($lines -match '^harness-error=') -or (Get-Date) -ge $deadline)
    $lines
    if ($lines -notcontains 'complete' -or $lines -notcontains 'account=RDAGENT-LAB\LabUser' -or
        $lines -notcontains 'administrator=False' -or $lines -notcontains 'exit=1' -or
        $lines -notcontains 'stdout=' -or !($lines -match '^stderr=.*PermissionDenied.*run enrollment as an elevated administrator')) {
        throw 'Ordinary-user CLI rejection was not demonstrated'
    }
    if ((Get-FileHash $state).Hash -ne $before -or (Get-Acl $state).Sddl -ne $acl) { throw 'State changed' }
    'PASS: ordinary-user CLI rejected; state hash and ACL unchanged'
} finally {
    if (Get-Service $name -ErrorAction SilentlyContinue) {
        Stop-Service $name -ErrorAction Continue
        sc.exe delete $name
        if ($LASTEXITCODE -ne 0) { throw 'Cannot remove probe service' }
    }
    Set-ServiceLogonRight $original
}
