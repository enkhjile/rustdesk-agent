# Management contract

Implemented by the local [contract fixture](contract-testing.md) and the
[Console fork integration](https://github.com/enkhjile/rustdesk-console/blob/main/docs/companion-agent.md).
This does not imply production deployment or support in upstream v0.2.40.
TLS with normal hostname/certificate validation is mandatory. Each bearer
credential identifies exactly one provisioned UUID; never authorize based on a
body or path UUID alone. Credentials must not appear in URLs, process arguments,
reports or logs. Console stores credential hashes and supports revocation; the
companion rotation change adds administrator-issued replacements. The in-process
fixture does not implement those administrator operations. Local provisioning
and replacement are documented in [enrollment.md](enrollment.md). Enrollment UI
remains unfinished. Limit requests/responses and enforce read/write deadlines.
No redirects. Fixed origin is provisioned locally by an administrator.

`GET /v1/agent/desired` with `Authorization: Bearer <device credential>` returns HTTP 200, content-type JSON:

```json
{"schema_version":1,"revision":7,"settings":{"approve_mode":"password","verification_method":"use-permanent-password"}}
```

The proposed protocol requires all objects to reject unknown or duplicate fields. Revision is a positive u64, monotonic per device, and denotes immutable content. Settings contains at least one of the two optional keys. Allowed values: approve_mode = `password`, `click`, `password-click`; verification_method = `use-temporary-password`, `use-permanent-password`, `use-both-passwords`. An omitted setting is unmanaged, not reset. Explicit null setting values are not part of the proposed protocol. No secret or command is accepted. A 204 means no assigned state; leave settings unchanged and retain the revision watermark. Do not apply cached state on a 204, authentication failure, invalid response or outage. 401/403 means stop applying and alert/retry at bounded intervals; 429/5xx/network errors retry with bounded backoff. Use a cap and jitter in the service, and honor a bounded Retry-After. No lower or equivocal revision may be applied, including after restart.

`POST /v1/agent/status` with the same authorization contains:

```json
{"schema_version":1,"agent_uuid":"a49f96c4-968b-4e7a-a3be-a5452385584d","rustdesk_id":"123456789","rustdesk_version":"1.4.7","desired_revision":7,"observation_sequence":1,"applied_revision":null,"results":{"approve_mode":"verified","verification_method":"mismatch"}}
```

`rustdesk_id` and `rustdesk_version` may be null when unavailable. `applied_revision` is null unless all settings in the current desired revision were independently read back as requested; `results` values are `verified`, `read_failed`, `write_failed`, `mismatch`, `unsupported`, `unverified_cli_match`, or `indeterminate`. The backend validates UUID against credential and accepts duplicate reports idempotently. `observation_sequence` is a positive u64 increasing per device across revisions. Identical retries of the latest report return 204; a lower sequence or changed content at the same sequence returns 409. The agent must persist this sequence before delivery; the fixture stores it only in memory. Reports for a revision other than the currently assigned revision return 409 (including null versus assigned). After an authenticated GET 204, a heartbeat uses the same status route with required desired_revision: null, applied_revision: null and results: {}. It retains the accepted revision watermark and uses the same persisted observation sequence and retry rules. Console accepts it only while no managed policy is assigned; assignment races return 409. Invalid or failed polls do not generate heartbeats. A heartbeat proves recent authenticated contact, not live RustDesk health or applied settings. Upgrade Console before the agent; old numeric-revision reports remain supported. Transient delivery failures are retried; 400/403/409 require correction or a fresh observation, not replaying the same rejected report. Managed polling reconciles on subsequent valid responses, including unchanged
revisions. Observe-only polling does not invoke application. New reports include
optional `management_mode`, either `managed` or `observe_only`; absence means an
older agent, not managed mode. Explicit null and unknown values are rejected.
Old pending reports retain their exact body when retried. Upgrade Console first.
Managed matching CLI output is `unverified_cli_match`; a persisted attempt
interrupted before its outcome is saved is `indeterminate`. Neither advances
applied revision. Other per-setting failures retain their distinct results.
Observe-only reports use `unsupported`. The Rust test fixture validates the full status schema.


## Current implementation limits

`parse_desired` checks schema version 1, a positive revision, at least one
non-null setting, allowed enum values and unknown fields. Callers should use this
function rather than deserialize `DesiredState` directly, which skips the semantic
checks. `reconcile` repeats the schema/revision/nonempty checks before any I/O.

Explicit null and duplicate fields are rejected. Machine-readable proposed
schemas are in [schemas/desired-state.schema.json](schemas/desired-state.schema.json)
and [schemas/status.schema.json](schemas/status.schema.json). JSON Schema cannot
detect duplicate keys after JSON parsing: the receiver must reject them while
parsing. It also cannot enforce revision history or compare applied_revision to
desired_revision; the backend must enforce those semantics. Results must match
the requested setting keys, and applied_revision must equal desired_revision
only when every result is verified. UUID format validation must be enabled.

`RevisionGuard::reconcile` tracks the highest accepted state in memory and rejects
stale or changed same-revision payloads before settings I/O. Reuse one guard per
device; the standalone `reconcile` function has no history. Failed application
does not lower the accepted revision. The guard itself is in-memory; the management client restores and commits the accepted state through the [protected state store](state.md).
The reconciliation core does not authenticate requests, persist state/reports or
schedule retries. The separate management client authenticates polling and optional managed application
and schedules bounded retries; see [service behavior](windows-service.md). The core backoff
helper returns 5, 10, 20, 40, 80, 160 and then 300 seconds for failure counts
starting at zero, without jitter or Retry-After handling. A matching initial read
is sufficient for `verified`; a successful write requires a matching second read.
The explicit local [setter](windows-service.md#explicit-local-setting-command) does not
participate in this protocol or advance applied revision. Its matching CLI readback
is unverified and must not be mapped to `verified`.
Choosing `use-permanent-password` selects a verification mode only; it does not
provision or change a password.
