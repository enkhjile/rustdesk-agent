# Windows service skeleton and read-only detection

This development slice targets Windows x64. It is not ready for kiosk deployment.
It implements SCM start/stop/shutdown, read-only HTTPS polling when enrolled, and
local diagnostic snapshots. There is no settings application. [Durable state and status delivery](state.md) require administrator initialization before starting an enrolled service. Local [enrollment and credential
storage](enrollment.md) are implemented but require Windows runtime validation.
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
is an administrator action, outside this implementation; installation was tested
only in the [isolated VM](windows-vm-validation.md). Do not rely on an installer/SCM default that silently selects
LocalSystem. The service account remains an explicit deployment decision.

The service probes and (when enrolled) polls immediately, then waits 60 seconds
after a successful cycle. Poll failures use the bounded retry delay described below.
STOP or SHUTDOWN interrupts the wait, HTTP request or active probe; cancellation requests
termination of the direct CLI child. Descendant processes are not managed by
a Windows Job Object in this skeleton. Snapshot-file writes are synchronous
and must finish before a pending stop can be handled; use a local report directory. SCM receives pending/running/stopped status.
Fatal runtime/report-write errors or an invalid existing enrollment store stop
the service with service-specific code 1. Missing enrollment keeps read-only
detection available; snapshots include `enrollment: null`. Valid enrollment adds
only UUID and management origin, never the credential. Missing enrollment also
sets `management: null` and makes no management requests.
Detection failures are recorded and retried on the next cycle. The JSON file
contains a timestamp and detection results; it is overwritten, not an audit log
or atomic durable revision store. Readers may need to retry during replacement.

## Read-only management polling

Valid enrollment enables `GET /v1/agent/desired` at the locally provisioned HTTPS
origin. The client uses the per-device bearer credential, validates certificates
and hostnames, disables redirects and automatic proxy discovery, and uses the same client for status POSTs. There is no insecure TLS or custom-CA command-line option. Request
headers containing the credential are marked sensitive; raw HTTP errors and
response bodies never enter diagnostics. HTTP/TLS libraries may retain memory
copies of headers; zeroization is not a guarantee against memory capture.

Connect timeout is 5 seconds and the whole request/body deadline is 10 seconds.
Only JSON 200 responses up to 64 KiB are parsed, with the existing strict schema
and persisted revision guard. A 204 means unassigned, retaining the watermark.
Invalid, stale and conflicting state is rejected. Acceptance does not verify or
apply any RustDesk setting. There is no settings adapter in the polling path.

Snapshots include `management.delivery` for status delivery and `management.status` (with a `kind` discriminator), the highest
`accepted_revision`, and `next_poll_seconds`. HTTP failures include only their
numeric status. Network/TLS failures use `transport_error`; deadlines use `timeout`.
All failures retry with exponential ceilings of 5, 10, 20, 40, 80, 160 and 300
seconds, jittered between 80% and 100% (minimum 5 seconds). On 429/5xx, valid
Retry-After seconds or HTTP dates raise that delay, capped at 300 seconds.
Successful GET 200/204 resets polling failures and uses a 60-second wait unless delivery is retrying. These waits
start after detection and polling finish; they are not a fixed wall-clock cadence.

The watermark and latest pending report now survive restart; see [state initialization,
permissions and recovery](state.md). Poll and delivery failures have separate retry
counters. If both fail, the cycle uses the larger delay. Storage failures stop the service. No cached state is applied during
outages, errors or unassignment; this service never writes settings at all.
To stop polling, stop the agent service. Do not delete enrollment as a toggle.

RustDesk 1.4.7's `--option` path requires installation and administrative privilege.
A LocalService probe is therefore expected to report invalid output for option
reads. An elevated administrator passes the source-level privilege check;
LocalSystem specifically is not required by that check. The isolated VM demonstrated prompt-free LocalService polling with RustDesk absent.
An account that can read installed RustDesk options remains unverified.

## Validation still required on disposable Windows

The [2026-11-08 validation record](windows-validation-2026-11-08.md) records local
Windows build/test results and the non-elevated storage-test failures. The host
reported physical Dell hardware; service installation and reboot testing were
therefore deferred to an isolated disposable VM.

The subsequent [isolated VM record](windows-vm-validation.md) records passing
elevated Windows tests, SCM/HTTPS reporting, service-token access, cancellation,
retries and guest reboot. Its pending section identifies the checks still outstanding.

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

Local verification: portable parser/process and HTTPS polling tests. The polling
slice adds a TLS dependency that requires Windows C headers, so the macOS
cross-target check cannot complete without the Windows SDK; use hosted Windows CI.
Actual read-only service/UAC results are recorded in the isolated VM report;
installed RustDesk runtime tests remain pending.

Sources: [RustDesk 1.4.7 CLI](https://github.com/rustdesk/rustdesk/blob/1.4.7/src/core_main.rs),
[IPC fallback/write behavior](https://github.com/rustdesk/rustdesk/blob/1.4.7/src/ipc.rs#L1664-L1697),
[windows-service](https://docs.rs/windows-service/0.8.1/windows_service/).

HTTP client reference: [reqwest request deadlines, redirects and TLS configuration](https://docs.rs/reqwest/0.13.5/reqwest/struct.ClientBuilder.html).
