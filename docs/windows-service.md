# Windows service skeleton and read-only detection

This development slice targets Windows x64. It is not ready for kiosk deployment.
It implements SCM start/stop/shutdown and local diagnostic snapshots only. There
is no enrollment, credential storage, management polling or settings application.
No service is installed automatically and no elevation is requested by the agent.

## Run a one-shot probe

Build on Windows with the MSVC build tools and SDK:

```powershell
cargo build --locked
& .\target\debug\rustdesk-agent.exe --detect 'C:\Program Files\RustDesk\rustdesk.exe'
```

Use a trusted installed executable at an explicit absolute path. Do not use a
user-writable RustDesk binary or directory for a privileged service. Binary
signature, installation provenance and ACL validation are still deployment gates;
the agent currently checks only path/file presence, not MSI registration or
Authenticode. It does not search PATH, run a shell or auto-install RustDesk.

The only child commands are `--version`, `--get-id`, `--option approve-mode` and
`--option verification-method`. Version is queried first; only exact `1.4.7`
continues to the other queries. Other versions are not automatically trusted.
Each command has a three-second timeout and a 4 KiB limit on each output stream.
Nonzero exits, nonempty stderr, malformed/blank output and timeouts are reported
without including raw command output in diagnostics. A missing executable is
reported as unavailable. Later fields are `unsupported_version` whenever the
version gate fails, including when version detection itself failed.

`observed` means output matched the expected format, not that live state was
verified. `live_state_verified` is always false. RustDesk can fall back to cached
configuration during IPC failures; even its getter commands can update local
caches internally. The agent issues no modifying CLI commands and never edits
RustDesk TOML. Selecting a password verification mode and delivering a permanent
password are separate operations; neither is implemented in this slice.

## Service entry point

Configure SCM with service name `RustDeskAgent` and this command line (substitute
administrator-controlled absolute paths; quote all paths):

```text
"C:\Program Files\RustDeskAgent\rustdesk-agent.exe" --service "C:\Program Files\RustDesk\rustdesk.exe" "C:\ProgramData\RustDeskAgent\detection.json"
```

The report path must be absolute and end in `.json`. Provision its directory first. Its ACL must permit the chosen service
account to write while excluding untrusted writers; protect the executable and
SCM configuration too. The skeleton does not set or verify these ACLs. Installation
is an administrator action, outside this implementation; no installation commands
have been run. Do not rely on an installer/SCM default that silently selects
LocalSystem. The service account remains an explicit deployment decision.

The service probes immediately and then waits 60 seconds between snapshots.
STOP or SHUTDOWN interrupts the wait or active probe; cancellation requests
termination of the direct CLI child. Descendant processes are not managed by
a Windows Job Object in this skeleton. Snapshot-file writes are synchronous
and must finish before a pending stop can be handled; use a local report directory. SCM receives pending/running/stopped status.
Fatal runtime/report-write errors stop the service with service-specific code 1.
Detection failures are recorded and retried on the next cycle. The JSON file
contains a timestamp and detection results; it is overwritten, not an audit log
or atomic durable revision store. Readers may need to retry during replacement.

RustDesk 1.4.7's `--option` path requires installation and administrative privilege.
A LocalService probe is therefore expected to report invalid output for option
reads. An elevated administrator passes the source-level privilege check;
LocalSystem specifically is not required by that check. No working unattended
account or prompt-free runtime behavior has yet been demonstrated.

## Validation still required on disposable Windows

- Run `--detect` with RustDesk absent, installed 1.4.7, a different version and
  the RustDesk service stopped. Compare reported ID/settings to the actual UI.
- Install under an explicitly selected test account; start, stop during a probe,
  restart and reboot. Verify SCM status and fresh timestamps, no orphan probe
  processes, no UAC prompts, and the expected privilege-related failures.
- Deny report-file access and confirm a nonzero stopped status. Check non-admin
  users cannot replace the binaries or report directory contents.
- Record Windows version, RustDesk binary hash/signature, account and results.
  Read-only success does not authorize enabling writes; IPC fallback and whole-map
  writes still require separate fault tests before a write adapter can ship.

Local verification: portable parser/process tests and Windows x64 cross-target
compile checks. Actual Windows service/UAC and RustDesk runtime tests are pending.

Sources: [RustDesk 1.4.7 CLI](https://github.com/rustdesk/rustdesk/blob/1.4.7/src/core_main.rs),
[IPC fallback/write behavior](https://github.com/rustdesk/rustdesk/blob/1.4.7/src/ipc.rs#L1664-L1697),
[windows-service](https://docs.rs/windows-service/0.8.1/windows_service/).
