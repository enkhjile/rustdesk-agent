# Windows installation

`scripts/install.ps1` provides administrator-driven Install, Upgrade and Uninstall
operations for Windows x64. Use **64-bit Windows PowerShell 5.1**, elevated. The
script does not request elevation, download executables, enroll a device or change
RustDesk settings. Builds use a static C runtime; no app-local Visual C++ runtime
DLLs are required by the agent.

See the [validation record](validation/windows-installer.md) for completed checks
and the remaining enrolled-service lifecycle gate.

Obtain the agent executable and its SHA-256 from a trusted build/distribution
channel. A matching hash checks integrity, **not publisher identity**. Executable
signing, distribution licensing/notices and a signed installer remain release
gates. This script is not an MSI package or production rollout authorization.

## Install

Install official RustDesk under an administrator-controlled Program Files path.
Then run, substituting the approved artifact and hash:

```powershell
.\scripts\install.ps1 -Action Install `
  -SourcePath 'C:\Staging\rustdesk-agent.exe' `
  -Sha256 '<trusted SHA-256>' `
  -RustDeskPath 'C:\Program Files\RustDesk\rustdesk.exe'
```

The installer uses fixed Windows Known Folder locations:

| Resource | Policy |
| --- | --- |
| Program Files\RustDeskAgent\rustdesk-agent.exe | Administrator-owned directory; SYSTEM/Administrators full control, agent service SID read/execute. |
| ProgramData\RustDeskAgent\detection.json | Protected report directory; SYSTEM/Administrators full control, agent service SID modify. OWNER RIGHTS prevents implicit permission-changing access through file ownership. |
| RustDeskAgent service | LocalService account, unrestricted service SID, automatic start, quoted executable/report paths, no `--lab-apply` flag. |

The new service is left **stopped**. Follow [enrollment](enrollment.md): run the
installed executable's `--enroll` with a device credential on redirected stdin,
bind its printed UUID in Console, and run administrator-only `--init-state` once.
Then `Start-Service RustDeskAgent`. Do not pass credentials to the installer or
put them in command arguments. Missing enrollment permits diagnostic mode;
invalid enrollment or missing initialized state fails closed as before.

The installer refuses existing installations, unexpected directory permissions,
reparse points, non-x64 payloads and hash mismatches. It does not take over a
manually configured service or repair weakened ACLs. RustDesk must be under
Program Files with trusted ownership and no untrusted write access.

## Upgrade

```powershell
.\scripts\install.ps1 -Action Upgrade `
  -SourcePath 'C:\Staging\rustdesk-agent.exe' `
  -Sha256 '<trusted SHA-256>'
```

Supply the same `-RustDeskPath` used during installation if it differs from the
default. The service path, account and SID must match. A protected staged copy is
hash-checked before stopping the service. The executable is replaced atomically
on the same volume. A previously running service is restarted; a stopped service
stays stopped. Automatic writes remain disabled even for a lab-capable binary.

Enrollment and durable-state directories are never opened, recreated, reset or
deleted by setup. Their UUID, credential, revision, sequence and pending report
remain available to the new service. Running the service can naturally advance
state afterward. Upgrades must use a compatible state schema; no downgrade or
schema-migration policy is implied.

Service stop has a 30-second deadline; setup does not kill an unresponsive service.
Failures return an error and may retain staged/previous executable files for
administrator inspection. If restart fails after replacement, the previous binary
is retained as `rustdesk-agent.previous`. No automatic rollback is attempted:
a new binary could already have changed durable state. Verify reporting in
Console after every upgrade; Running status alone is not an end-to-end health check.

## Uninstall and reinstall

```powershell
.\scripts\install.ps1 -Action Uninstall
```

Uninstall stops and removes the service, deletes only its known executable and
empty installation directory, and **retains enrollment, durable state and reports**.
Unexpected files stop removal; nothing is recursively deleted. There is no purge
option. Revoke the credential in Console separately when retiring a device.

A subsequent Install can reuse the protected report directory. Existing
enrollment/state remain intact; start the reinstalled service without reenrollment
or initialization. Do not clone these retained stores to another device.

For partial setup or interrupted upgrade, inspect the service, directory ownership
and leftover files before retrying. Setup refuses ambiguous installations instead
of silently overwriting them. Concurrent installer invocations are rejected.
