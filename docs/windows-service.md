# Windows service and local CLI commands

This development slice targets Windows x64. It is not ready for kiosk deployment.
It implements SCM start/stop/shutdown, read-only HTTPS polling by default, and
local diagnostic snapshots. Default builds do not apply settings; an explicit local
one-shot setter is described below. [Durable state and status delivery](state.md) require administrator initialization before starting an enrolled service. Local [enrollment and credential
storage](enrollment.md) passed the isolated VM access and lifecycle checks.
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

The detector's only child commands are `--version`, `--get-id`, `--option approve-mode` and
`--option verification-method`. Version is queried first; only exact `1.4.7`
continues to the other queries. Other versions are not automatically trusted.
Each command has a three-second timeout and a 4 KiB limit on each output stream.
Nonzero exits, nonempty stderr, malformed output and timeouts are reported
without including raw command output in diagnostics. A missing executable is
reported as unavailable. Later fields are `unsupported_version` whenever the
version gate fails, including when version detection itself failed.

For version and ID, `observed` means output matched the expected format, not that
live state was verified. Settings use `unverified_cli_value` with the allowlisted
value when nonblank, or `ambiguous_default` without a value when blank. Empty
version/ID output remains `invalid_output`. No default value is inferred from
blank option output. `live_state_verified` is always false. These local diagnostic
statuses do not change management reports: results remain `unsupported` and
applied revision remains null. RustDesk can fall back to cached
configuration during IPC failures; even its getter commands can update local
caches internally. The detector issues no modifying CLI commands. The agent never
edits RustDesk TOML or provisions permanent passwords.

## Experimental managed application

For isolated lab validation only, build with `cargo build --locked --features lab-apply`
and append `--lab-apply` to the existing `--service <RustDesk.exe> <report.json>`
arguments. Both gates are required; default builds reject the flag. Enrollment,
TLS validation, service-SID access and durable state requirements remain in force.
Application accepts only loopback IP origins or `localhost`, pinned to `127.0.0.1`.
Other management origins are rejected before management requests or application.

Each fresh, valid desired response is revision-checked and durably saved before
settings I/O. The adapter checks official version 1.4.7, reads each requested
setting and skips a write when CLI output already matches. Different or ambiguous
values invoke this executable's existing `--set-option` worker, with its global
lock, allowlist and job containment. Failed reads do not trigger writes. Omitted
keys stay unmanaged. Equal revisions repeat observation and can repair CLI-visible
drift; a failed setting does not erase another setting's outcome.

Results are `unverified_cli_match`, `read_failed`, `write_failed`, `mismatch` or
`unsupported`. `indeterminate` is persisted before I/O and remains possible after
interruption. Per-setting outcomes are saved before proceeding to the next key.
`applied_revision` is always null: CLI output may be cached, including on offline
RustDesk. No result is promoted to `verified`. A later valid poll may retry failed
application; delivery retries alone never reapply cached desired state.
The local diagnostic detection snapshot precedes application in that cycle.

The service and worker use nested process-lifetime jobs so stopping or killing
the service cleans up its workers and their descendants. A canceled command may
already have changed settings. There is no transactional application or rollback,
and other writers do not participate in the agent lock. Production application
remains disabled; this experiment does not satisfy the live-verification gate.

## Explicit local setting command

For an authorized local change using the official installed RustDesk 1.4.7:

```powershell
& .\target\debug\rustdesk-agent.exe --set-option 'C:\Program Files\RustDesk\rustdesk.exe' approve-mode click
```

Only `approve-mode` (`password`, `click`, `password-click`) and
`verification-method` (`use-temporary-password`, `use-permanent-password`,
`use-both-passwords`) are accepted. Selecting a verification mode does not create
a password. Empty values, other settings and arbitrary arguments are rejected.
Use the same trusted absolute executable path requirements as the detector.

The command checks version, invokes one setter, then reads the setting back, with
the same three-second deadline and 4 KiB stream limits for each subprocess.
It never requests elevation, starts a shell, retries or rolls back automatically.
Nonzero exit, stderr or non-whitespace setter stdout fail the command, including
RustDesk diagnostics emitted with exit zero. Failure or cancellation may occur
after a setting changes; inspect the result before manually retrying.

JSON separates `command_accepted` from `readback_matches`. The readback retains
its `unverified_cli_value`/`ambiguous_default`/failure status and
`live_state_verified` remains false because CLI fallback is possible. Exit zero
means acceptance plus matching CLI readback, not verified connection behavior.
A readback mismatch prints the report and exits nonzero. This command is not
connected to polling or reconciliation; management still reports `unsupported`
with null applied revision. See [isolated setter validation](validation/2026-11-09-official-cli-setters.md).

Explicit `--set-option` invocations now share a machine-wide Windows mutex across
sessions and accounts. It covers the version check, write and CLI readback for
both allowed keys, regardless of executable path. A contender waits at most
three seconds for the lock, then exits nonzero with `Settings busy` before
launching a RustDesk command. Lock creation/access errors also fail closed.
The mutex grants synchronization/release access to its creator, administrators,
SYSTEM and LocalService; it is not a RustDesk authorization boundary.

If Windows reports abandoned ownership, the invocation exits nonzero and asks
the operator to inspect settings before retrying. This does not roll back a
partial write.
The lock is specific to this CLI entry point: older agents, direct library calls,
RustDesk GUI and other writers do not participate. Mismatch detection remains
necessary, and automatic retries/writes are still disabled.

Before launching RustDesk, the explicit setter also joins an unnamed,
non-inheritable Windows Job Object configured to terminate member processes when
its last handle closes. The CLI retains that handle until OS process teardown,
so normal exit or forced termination also cleans up its child process tree.
Membership is established before spawning, avoiding a spawn-then-assign gap.
Containment setup errors fail before launching RustDesk, including incompatible
parent-job restrictions. Existing RustDesk service/GUI processes are not added
to this job. Default builds install this containment only for `--set-option`;
the experimental lab service also contains its workers.
See [Windows Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects).

In the pre-serialization
[concurrency test](validation/2026-11-09-concurrent-settings.md), overlapping
changes to the two allowed keys lost one requested update in 4/20 rounds; each
affected invocation detected a readback mismatch and exited nonzero. Inspect
both settings after a mismatch rather than assuming the other change survived.

The [unavailable-process test](validation/2026-11-09-unavailable-processes.md)
confirmed that setters can return matching readback while the RustDesk service
is disabled and background processes are absent. Offline results differed by
account; LocalService changes appeared in both accounts' getters after restart.
Do not treat success as proof of live application or assume an offline command
has no effect on a later service start.

A separate direct IPC reader is not a supported workaround for 1.4.7: the
[Windows main IPC authorization](https://github.com/rustdesk/rustdesk/blob/1.4.7/src/ipc/auth.rs#L660)
requires the peer executable to match RustDesk, while the
[CLI options reader](https://github.com/rustdesk/rustdesk/blob/1.4.7/src/ipc.rs#L1721)
hides connection failures behind cache fallback. Independent live-read provenance
remains unresolved. This does not block the tested official CLI setters or require
a custom client; the upstream proposal #11 was closed as superseded by #12.
The agent does not impersonate RustDesk, inject code or treat cache/TOML values
as live settings.

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
only in the [isolated VM](validation/2026-11-08-service-lifecycle.md). Do not rely on an installer/SCM default that silently selects
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

## Validation status

The [isolated VM service run](validation/2026-11-08-service-lifecycle.md) passed
service-SID access, ordinary-user/unrelated-service denial, administrator
initialization, HTTP cancellation/retries and unattended reboot as LocalService.
The [installed-client run](validation/2026-11-08-rustdesk-1.4.7-getters.md) additionally
verified cancellation of a stalled real CLI child.

Early blank option results under both accounts were not proof of a LocalService
permission failure. The [getter/GUI diagnosis](validation/2026-11-08-getter-diagnosis-and-gui.md)
showed that defaults can produce blank CLI output. The subsequent
[actual agent setter run](validation/2026-11-09-official-cli-setters.md) successfully
changed both settings as administrator and LocalService, with matching reads from
the other account and retained values after service restart and VM reboot.
Original values were restored and the VM shut down. No guest UAC interaction
was needed for those commands.

Remaining tests include the other two allowlisted mode values, independent GUI
and connection verification, disabled/unavailable RustDesk, whole-map/concurrent
changes and cancellation during a real setter. Deployment also needs binary and
report-directory protection, report-write failure handling and the other gates in
the [current plan](plan.md). The explicit setter does not enable automatic service
application or claim verified live state.

Use the [validation index](validation/README.md) for dated environments, hashes,
commands and limitations. Historical reports retain their original observations;
for example, early agents reported blank options as `invalid_output`, whereas the
current detector reports `ambiguous_default`.

Sources: [RustDesk 1.4.7 CLI](https://github.com/rustdesk/rustdesk/blob/1.4.7/src/core_main.rs),
[IPC fallback/write behavior](https://github.com/rustdesk/rustdesk/blob/1.4.7/src/ipc.rs#L1664-L1697),
[windows-service](https://docs.rs/windows-service/0.8.1/windows_service/).

HTTP client reference: [reqwest request deadlines, redirects and TLS configuration](https://docs.rs/reqwest/0.13.5/reqwest/struct.ClientBuilder.html).
