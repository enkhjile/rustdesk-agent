# Isolated Windows VM validation — 2026-11-08

> Historical validation record: results and pending checks describe the tested
> revision and date, not current project status. See the [current plan](../plan.md)
> and [validation index](README.md) for subsequent results.

This record follows the [host-only attempt](windows-validation-2026-11-08.md).
It tests the read-only executable built from `0534dcab782307bddf86f41a51937577c9ea3124`
on PR #5. No agent code or RustDesk settings were changed.

## Environment

- Hyper-V generation 2 VM `RustDeskAgent-Validation`, guest `RDAGENT-LAB`.
- Four virtual CPUs, 4 GiB RAM, 64 GiB dynamic OS disk, vTPM, **no network adapter**.
- Windows 11 Pro 25H2, `10.0.26200.6584`, installed from the host's existing Japanese 25H2
  ISO, image index 3. Setup account: `RDAGENT-LAB\LabAdmin` (elevated).
- Agent account: `NT AUTHORITY\LocalService`, service SID enabled/unrestricted.
  Agent SID: `S-1-5-80-312778130-3777190381-3969750968-319843892-2371153462`.
- RustDesk intentionally absent: the service receives `C:\Lab\missing-rustdesk.exe`.
  Getter/IPC behavior against an installed RustDesk is not validated here.
- Dummy credentials only. Test endpoint: `https://localhost:8443`, TLS listener
  bound to `127.0.0.1`. A seven-day localhost certificate is trusted only in the
  guest's LocalMachine root store; TLS verification in the agent is unchanged.

The existing Rust fixture remains in-process and is exercised by the contract
tests. For SCM testing, a separate throwaway PowerShell TLS listener runs as a
guest SYSTEM startup task. An administrator-controlled JSON file selects desired
revision, response codes and response delays. It logs request times and status
bodies, never Authorization headers. It is a fault-injection endpoint, not a
production backend or a replacement for the contract/schema tests.

## Executed setup and tests

The host's pinned Rust 1.99.1 formatting, Clippy and Windows x64 build results are
recorded in the earlier report. The resulting five test executables were copied
into `C:\Lab\tests` and executed under the elevated guest account:

```powershell
foreach ($test in Get-ChildItem C:\Lab\tests\*.exe) {
    & $test.FullName
    if ($LASTEXITCODE -ne 0) { throw 'Test failed' }
}
```

All **48 tests passed**: 18 library, 1 executable, 6 contract, 5 enrollment and
18 reconciliation tests. This includes both protected-storage tests that could
not run under the non-elevated host account. The compiler/build ran on the host;
this is execution of its test binaries on Windows, not a guest Cargo installation.

Guest provisioning commands (dummy token supplied through redirected stdin):

```powershell
sc.exe create RustDeskAgent binPath= '"C:\Lab\rustdesk-agent.exe" --service "C:\Lab\missing-rustdesk.exe" "C:\ProgramData\RustDeskAgent\detection.json"' start= auto obj= 'NT AUTHORITY\LocalService'
sc.exe sidtype RustDeskAgent unrestricted
New-Item -ItemType Directory C:\ProgramData\RustDeskAgent
icacls.exe C:\ProgramData\RustDeskAgent /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' 'NT SERVICE\RustDeskAgent:(OI)(CI)M'
# Pipe the public dummy fixture token to:
# C:\Lab\rustdesk-agent.exe --enroll https://localhost:8443
C:\Lab\rustdesk-agent.exe --init-state
Start-Service RustDeskAgent
sc.exe queryex RustDeskAgent
Get-Content C:\ProgramData\RustDeskAgent\detection.json
Get-Content C:\ProgramData\RustDeskAgentState\state.json
```

Administrator initialization succeeded. SCM reported RUNNING with exit codes 0.
The agent accepted revision 7, persisted sequence 1, posted `unsupported` with
null applied revision and received 204. Committed state then had `pending: null`.
The replacement state file owner was `NT AUTHORITY\LOCAL SERVICE`.

## Service-token access checks

After the real agent replaced state, it was stopped. A small .NET service helper
temporarily ran as `RustDeskAgent`, then as `RustDeskLabOther`, both LocalService
with their respective service SIDs enabled. The original agent command line was
restored. The helper calls `CreateFileW` with specific access masks and closes the
handle; it never reads enrollment bytes or changes data/ACLs. Error 5 means access
denied; 0 means the handle was granted.

| Requested access | Agent SID | Other service, same account | Ordinary user |
| --- | --- | --- | --- |
| Enrollment GENERIC_READ | 0 | 5 | 5 |
| Enrollment GENERIC_WRITE | 5 | 5 | 5 |
| State GENERIC_READ | 0 | 5 | 5 |
| State GENERIC_WRITE | 0 | 5 | 5 |
| State WRITE_DAC | 5 | 5 | 5 |
| State WRITE_OWNER | 5 | 5 | 5 |

This verifies denial of permission-changing handle rights even when the other
service shares the replacement file owner's account. Both token group lists
were recorded, including their distinct service SIDs. It is not an assertion
against administrators, SYSTEM or a compromised agent.

## HTTP cancellation, retry and reboot results

These checks completed on the real SCM service, not an in-process poller:

| Check | Observed result |
| --- | --- |
| Pending report after a 503 | Revision 7, sequence 2 persisted before stop |
| Restart with GET 204 / POST 503 | Three byte-identical POST attempts; revision 7 and sequence 2 retained |
| Retry request spacing | 5.67 s then 9.62 s, including request overhead |
| Stop during an eight-second POST delay | STOPPED in 252 ms; Win32/service exit codes 0 |
| Stop during an eight-second GET delay | STOPPED in 251 ms; Win32/service exit codes 0 |
| POST 204 acknowledgement | Pending report cleared durably |
| Fresh observation before reboot | Revision 7, sequence 3, pending report retained after POST 503 |
| Guest reboot | LastBootUpTime `2026-11-08T18:22:56.9498890+09:00`; service automatically RUNNING as LocalService, exit codes 0 |
| Report after reboot | Same sequence-3 body acknowledged at `18:23:09+09:00`; committed state retained revision 7/sequence 3 and had `pending: null` |

The harness changed only its own `C:\Lab\control.json`, used `Start-Service` and
`Stop-Service RustDeskAgent`, checked `sc.exe queryex RustDeskAgent`, and read
committed state plus request logs. Reboot command inside the guest was
`shutdown.exe /r /t 5`. The endpoint and agent started after reboot without an
interactive guest login or UAC approval. Host-side UAC approvals were required
for the test orchestrator; they are distinct from guest service behavior.
The final check confirmed the endpoint listened on `127.0.0.1:8443`, then stopped
both the agent service and endpoint task.

Executable SHA-256, independently read inside the guest:
`6E11A0A2FEF767B2DF3E74A572238565743C5F1BBAB3C54341D56D50075E7A2C`.

## Ordinary-user checks and retained artifacts

Ordinary-user file-access checks completed at `19:18:06+09:00`. The harness used
`LogonUserW` (batch logon, type 4), then `WindowsIdentity.Impersonate` and the same
`CreateFileW` access masks as the service helper. The recorded identity was
`RDAGENT-LAB\LabUser`, SID `S-1-5-21-3490330849-2768354856-2119030319-1002`.
Its groups included Users and Batch, not Administrators or the agent service SID.
All six requests returned error 5. This exercises actual Windows access checks
under an ordinary-user token; it does not claim an interactive-user CLI run.

Repeated elevated initialization also passed: the following command returned 1,
and SHA-256 of `state.json` before and after was identical:

```powershell
cmd.exe /c 'C:\Lab\rustdesk-agent.exe --init-state > C:\Lab\Results\repeat-init.stdout 2> C:\Lab\Results\repeat-init.stderr'
```

The ordinary-user `--init-state` CLI check passed at `19:40:57+09:00` using
[the retained manual harness](../../tests/manual/user-init.ps1). It runs a temporary
SCM helper as `RDAGENT-LAB\LabUser`, which starts the actual agent executable with
`--init-state` and redirected stdout/stderr. The recorded token included Users
and Service, with `administrator=False`, and no agent service SID. Results:

```text
account=RDAGENT-LAB\LabUser
administrator=False
exit=1
stdout=
stderr=Error: Custom { kind: PermissionDenied, error: "run enrollment as an elevated administrator" }
PASS: ordinary-user CLI rejected; state hash and ACL unchanged
[SC] DeleteService SUCCESS
```

This proves rejection by the agent's administrator check under an ordinary-user
service logon. It does not claim an interactive desktop logon test. No agent code
change was needed. The temporary service was removed, and the original
`SeServiceLogonRight` assignment was restored. The helper has no credentials
embedded in its source; the guest-only runner supplied the existing dummy
credential as a `PSCredential`.

The earlier scheduled-task attempts failed or remained queued; direct
`CreateProcessWithTokenW` children failed during initialization (`0xC0000142` or
`0x8007045A`). Using a normal SCM-created user session bypassed that failing launch
path. The precise cause of those initialization failures was not established.
An initial service-logon policy attempt also failed; diagnostic output was added,
and the subsequent run completed. These harness failures are not agent failures.

To reproduce in the already provisioned disposable guest, copy the manual script
there, then run in elevated PowerShell (archive previous probe output first):

```powershell
$dummy = Get-Credential 'RDAGENT-LAB\LabUser'
& C:\Lab\user-init.ps1 -Credential $dummy
```

Prerequisites are the existing lab account, executable, enrolled/initialized
state, endpoint task and `C:\Lab\Results` writable by LabUser. The script refuses
other machine/account names and an existing helper service or result file. It
stops the agent and endpoint before testing, grants only temporary service-logon
rights, captures real CLI output, and checks the state hash and ACL. It is manual
lab tooling, not an installer or part of the shipped agent.

For this follow-up, pinned-toolchain formatting, Clippy and Windows x64 build
passed again. All 49 non-administrator tests passed outside the network sandbox;
the sandbox itself blocked loopback HTTPS. The two administrator-only tests
retain the earlier passing guest results above; they were not rerun in this
follow-up. The new manual harness compiled and passed inside the guest.

Host scripts and raw transcripts remain under `target\validation\`; guest tools
and logs are under `C:\Lab`. These local harness files are not shipped agent
features. Final cleanup ran `Stop-VM -Name RustDeskAgent-Validation` after checking that it still had no network adapters. Hyper-V confirmed its state was `Off`. The agent and endpoint had already been stopped after reboot validation. No production
endpoint, real credential, RustDesk write, merge or release was involved.

Remaining separate gates include installed RustDesk getter/IPC behavior and stop
during an active CLI child, physical power-loss durability, and any future write
adapter. This read-only run does not authorize settings writes or deployment.

Setup issues were confined to the harness: host-side BCD hive collision (resolved
by repair inside Windows PE), copied read-only WIM, automatically inserted MSR
partition, unavailable 8 GiB allocation, automatic checkpoint disk paths and the
guest's default script execution policy. The final VM uses 4 GiB and automatic
checkpoints are disabled. No agent implementation failure has been demonstrated.
