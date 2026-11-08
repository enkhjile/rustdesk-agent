# Isolated Windows VM validation — 2026-11-08

This record follows the [host-only attempt](windows-validation-2026-11-08.md).
It tests the read-only executable built from `0534dcab782307bddf86f41a51937577c9ea3124`
on PR #5. No agent code or RustDesk settings were changed.

## Environment

- Hyper-V generation 2 VM `RustDeskAgent-Validation`, guest `RDAGENT-LAB`.
- Four virtual CPUs, 4 GiB RAM, 64 GiB dynamic OS disk, vTPM, **no network adapter**.
- Windows 11 Pro, `10.0.26200`, installed from the host's existing Japanese 25H2
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

| Requested access | Agent SID | Other service, same account |
| --- | --- | --- |
| Enrollment GENERIC_READ | 0 | 5 |
| Enrollment GENERIC_WRITE | 5 | 5 |
| State GENERIC_READ | 0 | 5 |
| State GENERIC_WRITE | 0 | 5 |
| State WRITE_DAC | 5 | 5 |
| State WRITE_OWNER | 5 | 5 |

This verifies denial of permission-changing handle rights even when the other
service shares the replacement file owner's account. Both token group lists
were recorded, including their distinct service SIDs. It is not an assertion
against administrators, SYSTEM or a compromised agent.

## Pending checks and retained artifacts

Ordinary-user validation is not yet complete. Launching the helper with
`Start-Process -Credential` through PowerShell Direct did not produce a successful
harness result. A later limited scheduled-task attempt registered successfully,
but returned `267011` (`0x41303`, not yet run) and produced no probe output within
20 seconds. That is a harness prerequisite failure, not evidence of user access
or denial. A diagnostic phase is prepared to inspect task settings and retry;
Windows canceled its host UAC launch before it started. Start/stop during GET/POST, report retries across
restart, guest reboot persistence, repeated/ordinary-user initialization and
prompt-free operation after setup are still pending.

Host scripts and raw transcripts remain under `target\validation\`; guest tools
and logs are under `C:\Lab`. These local harness files are not shipped agent
features. `runtime-validation.ps1` is the next elevated host step; it diagnoses
the limited task and then runs the independent HTTP/reboot checks. The VM remains
isolated and running; the agent was stopped after the access probes. No production
endpoint, real credential, RustDesk write, merge or release was involved.

Setup issues were confined to the harness: host-side BCD hive collision (resolved
by repair inside Windows PE), copied read-only WIM, automatically inserted MSR
partition, unavailable 8 GiB allocation, automatic checkpoint disk paths and the
guest's default script execution policy. The final VM uses 4 GiB and automatic
checkpoints are disabled. No agent implementation failure has been demonstrated.
