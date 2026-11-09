# Durable management state and status delivery

The service now keeps the accepted desired state, observation sequence and latest
pending status in one protected snapshot. Automatic service writes remain disabled. Every
requested setting is reported as `unsupported`, with `applied_revision: null`;
this means live verification/application is not enabled, even if a CLI getter
appears to match. Detected RustDesk ID/version are reported when available.
No credential is stored in the state file or included in a report.

## Initialization and upgrade

After installing the service with its service SID enabled and completing local
enrollment, an elevated administrator runs:

```powershell
& 'C:\Program Files\RustDeskAgent\rustdesk-agent.exe' --init-state
```

Do this before starting an enrolled service on this version, including upgrades
from the earlier in-memory polling slice. The command loads the existing enrolled
UUID/origin and exclusively creates the Windows Known Folder ProgramData directory
`RustDeskAgentState`. It never adopts or overwrites an existing directory. It does
not prompt for elevation, generate identity, change credentials, contact a server,
or install/start the service. A partial initialization requires investigation;
rerunning cannot overwrite it. Administrator initialization and service persistence
were exercised only in the [isolated VM](validation/2026-11-08-service-lifecycle.md); this is not
deployment approval.

The snapshot is bound to both enrolled UUID and management origin. Missing,
invalid, oversized, inaccessible or mismatched state stops enrolled service startup.
Missing enrollment still permits local detection without network requests.
There is no automatic reset, import, repair or recovery command.

## Storage boundary

The directory has an administrator owner and protected DACL: SYSTEM and
Administrators have full control; the service SID has read/write/execute/delete
rights. State, staging and lock files have the same protected DACL, with ownership
defaulting to the creating account. An OWNER RIGHTS entry grants only read-control
through ownership, suppressing implicit permission-changing rights; sharing the
service account alone must not grant another service control of these files. This lets a non-administrator service create
replacement files without claiming administrator ownership. The loader verifies
the directory owner/DACL, file DACLs and absence of reparse points. A held directory
handle prevents directory replacement; an exclusive file lock prevents two agents
from updating state simultaneously. The system ProgramData parent must retain
its administrator-managed protections. Local administrators and the agent itself
remain trusted; this does not defend against their tampering or restored old disks.
Enrollment permissions are unchanged and remain read-only through the service SID.

`state.json` is bounded to 16 KiB. Saves create `state.next` exclusively, write and
flush it, then replace `state.json` on the same volume with `MoveFileExW` using
replace-existing and write-through flags. Staged bytes are never loaded as state.
A leftover staging file is verified and removed on the next save while holding
the writer lock. A save error poisons the current poller and stops the service;
no later report can be sent from that process. Startup reloads only committed
state. Tests simulate interrupted staging and acknowledgement persistence failures;
physical power-loss and filesystem durability still need disposable Windows testing.

## Delivery and ordering

A validated authenticated 200 response atomically saves its accepted revision,
next sequence and new pending report before that report may be sent. Lower or
conflicting revisions remain rejected across restarts. Sequence overflow is
rejected; it never wraps. A 204 or failed poll retains the watermark.

Each cycle first retries an existing pending report byte-for-byte, then polls for
a fresh desired state. At most one status POST is made per cycle. If no pending
report was attempted, a newly saved report is sent in the same cycle. Otherwise,
the new observation replaces the pending slot and is sent next cycle. There is
only one pending report, not an unbounded queue. UUID/sequence ordering follows
the existing test-backend contract.

A 204 acknowledgement clears the outbox durably. If that save fails, restart
retries the original report, which the backend accepts idempotently. HTTP 400,
403 and 409 clear the rejected item durably; only a fresh valid observation can
produce a replacement. Other HTTP/network failures retain it and retry with
bounded backoff/jitter and capped Retry-After on 429/5xx. Delivery has its own
failure counter; successful GETs do not reset it. If polling and delivery both
fail, the larger delay is used. A terminal delivery rejection is visible in the
local snapshot; it is not an acknowledgement of applied settings.

Requests share the enrolled HTTPS origin, authentication, TLS validation,
redirect/proxy restrictions and timeouts. Service shutdown cancels either network
request or retry wait. Small synchronous state/snapshot file operations must
finish before shutdown is handled. There are no reports without a desired
revision; unassigned-device/network diagnostics stay local.

## Recovery and rollback

Stop the service before administrator maintenance. Investigate corrupt/missing
state and coordinate recovery with backend revision/sequence history; restoring
an older snapshot can violate ordering. Do not delete state to make startup pass.
Reprovisioning with a new identity requires a separately authorized backend and
credential operation. No such operation is implemented or performed here.

Stopping or downgrading the binary does not alter RustDesk settings. Preserve the
state directory when rolling back. The earlier polling-only binary ignores it;
returning to this version reloads it. Enabling automatic service writes remains blocked on the
remaining live-verification and end-to-end gates in the [current plan](plan.md).
The explicit local setter does not update management revisions or status.

References: [MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw),
[Windows file security](https://learn.microsoft.com/en-us/windows/win32/fileio/file-security-and-access-rights).

[OWNER RIGHTS semantics](https://learn.microsoft.com/en-us/windows-server/identity/ad-ds/manage/understand-security-identifiers).
