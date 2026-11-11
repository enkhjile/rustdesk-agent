# Console-driven RustDesk password changes

This branch adds a per-device permanent-password action for official RustDesk
1.4.7 on Windows x64. It is separate from approval and password-type policy:
changing a password does not select permanent-password authentication or change
connection approval. Observe-only agents never claim password operations.

The supported execution context is the installed `RustDeskAgent` Windows service
running as LocalService with its service SID enabled. Other accounts and client
versions report `unsupported`. The executable is the same locally configured,
administrator-protected official RustDesk executable used for settings management.

## Delivery and recovery

Console requires an administrator to queue a replacement for one bound, active
device. Passwords are 8–128 printable ASCII characters. Console encrypts the
pending payload with its separately provisioned password key; ordinary device,
policy, report and operation-status responses contain no password.

The agent uses its existing enrolled HTTPS origin and device credential:

- `POST /v1/agent/password/claim` returns 204 when no operation is pending, or a
  schema-version-1 object with `operation_id` (UUID v4), positive numeric
  `sequence`, `expires_at` (Unix seconds), and `password`.
- `password: null` is an expired/removed-secret tombstone. It is acknowledged
  without executing a command. The field is required even for tombstones.
- `POST /v1/agent/password/result` sends `schema_version`, `operation_id`,
  `sequence` and `outcome`; identical terminal receipts are idempotent. No secret
  is present. A successful acknowledgement returns 204.
  A receipt older than the current server operation also returns 204 without
  changing that operation, allowing recovery from a lost acknowledgement after
  an administrator queues a replacement.

The protected state file retains only operation ID, sequence, outcome and
acknowledgement state. The agent durably records `indeterminate` before invoking
the official command. After interruption it retries that result, never the
password command. A newer operation cannot overtake an unresolved operation.
Storage failure prevents execution or stops the service; it does not silently
reset history. Acknowledged sequence history remains across restarts.

Console erases expired ciphertext with a periodic cleanup while running. A claimed
operation retains metadata until the agent acknowledges an outcome, even after
secret expiry. This deliberately prevents replacement work from overtaking a
possibly interrupted command. If the response to a claim was lost, the agent can
later receive the non-secret tombstone and resolve it. Restore missing service
connectivity or state using the existing recovery procedure rather than deleting
the operation record. Revocation removes queued ciphertext; rotating the device
credential retains operation history.

| Outcome | Meaning |
| --- | --- |
| `command_accepted` | Official CLI returned exit 0, exactly `Done!` after whitespace trimming, and empty stderr. No independent connection verification is performed. |
| `indeterminate` | Execution may have been interrupted or its outcome could not be established. Do not automatically replay. |
| `expired` | Payload expired or was removed before this agent attempted execution. |
| `unsupported` | Client version, executable availability or service identity does not meet the supported execution requirements. |

The command shares the settings-command lock and existing job containment. Service
stop cancels the child. Neither password handling nor an acknowledgement advances
the settings policy's applied revision.

Failed claims and result acknowledgements share an independent in-memory failure
counter with bounded exponential backoff and jitter (5–300 seconds). HTTP 429 and
5xx responses also honor numeric or HTTP-date `Retry-After`, capped at 300 seconds.
Malformed headers are ignored. A claim returning 204 or an acknowledged result
resets this counter; successful settings polls do not. The service waits for the
larger of the settings/report delay and the password retry delay, so healthy
settings polling keeps a 60-second minimum. Restart resets retry timing, but
retains the durable operation history and pending acknowledgement.

## Secret boundary and validation

The official client accepts the password as a process argument. VM checks denied
ordinary-user and unrelated-service access to the tested service and child
processes. Administrators, SYSTEM and privileged process monitoring can still
observe arguments or memory. Do not configure application/proxy request-body
logging for the password endpoints; privileged endpoint telemetry needs its own
retention policy. Deleting ciphertext does not erase earlier database backups.

The [command validation record](validation/password-command.md) proves replacement
and reboot persistence using dummy passwords and official RustDesk. The
[integrated run](validation/password-management.md) passed service execution,
acknowledgement recovery, observe-only and reboot-history checks.
Deploy the supporting Console before upgrading agents. Downgrading
to an agent that predates password-operation state requires explicit state
compatibility review; do not discard the execution history to force a downgrade.
