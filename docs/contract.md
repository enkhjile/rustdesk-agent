# Management contract (test backend only)

Implemented only by the local [contract fixture](contract-testing.md), not an existing Console API or production service. TLS with normal hostname/certificate validation is mandatory. Each bearer credential identifies exactly one provisioned UUID; never authorize based on a body or path UUID alone. Credentials must not appear in URLs, process arguments, reports or logs. The production backend must retain credential hashes and support rotation/revocation and audit; these are not implemented by the test fixture. Local provisioning is documented in [enrollment.md](enrollment.md); backend enrollment and authorization UX remain to be designed. Limit requests/responses and enforce read/write deadlines. No redirects. Fixed origin is provisioned locally by an administrator. These routes and JSON are proposals, not claims about v0.2.40.

`GET /v1/agent/desired` with `Authorization: Bearer <device credential>` returns HTTP 200, content-type JSON:

```json
{"schema_version":1,"revision":7,"settings":{"approve_mode":"password","verification_method":"use-permanent-password"}}
```

The proposed protocol requires all objects to reject unknown or duplicate fields. Revision is a positive u64, monotonic per device, and denotes immutable content. Settings contains at least one of the two optional keys. Allowed values: approve_mode = `password`, `click`, `password-click`; verification_method = `use-temporary-password`, `use-permanent-password`, `use-both-passwords`. An omitted setting is unmanaged, not reset. Explicit null setting values are not part of the proposed protocol. No secret or command is accepted. A 204 means no assigned state; leave settings unchanged and retain the revision watermark. Do not apply cached state on a 204, authentication failure, invalid response or outage. 401/403 means stop applying and alert/retry at bounded intervals; 429/5xx/network errors retry with bounded backoff. Use a cap and jitter in the service, and honor a bounded Retry-After. No lower or equivocal revision may be applied, including after restart.

`POST /v1/agent/status` with the same authorization contains:

```json
{"schema_version":1,"agent_uuid":"a49f96c4-968b-4e7a-a3be-a5452385584d","rustdesk_id":"123456789","rustdesk_version":"1.4.7","desired_revision":7,"observation_sequence":1,"applied_revision":null,"results":{"approve_mode":"verified","verification_method":"mismatch"}}
```

`rustdesk_id` and `rustdesk_version` may be null when unavailable. `applied_revision` is null unless all settings in the current desired revision were independently read back as requested; `results` values are `verified`, `read_failed`, `write_failed`, `mismatch`, or (future adapter) `unsupported`. The backend validates UUID against credential and accepts duplicate reports idempotently. `observation_sequence` is a positive u64 increasing per device across revisions. Identical retries of the latest report return 204; a lower sequence or changed content at the same sequence returns 409. The agent must persist this sequence before delivery; the fixture stores it only in memory. Reports for a revision other than the currently assigned revision return 409. Heartbeat/error reporting without a desired revision also remains to be specified; the current schema covers reconciliation reports only. Transient delivery failures are retried; 400/403/409 require correction or a fresh observation, not replaying the same rejected report. Subsequent polls also reconcile actual state, including unchanged revisions. The Rust agent currently serializes only the reconciliation portion; local identity and credential storage are implemented separately, but durable revision/report state, HTTP and report transport are not implemented. The Rust test fixture validates the full status schema.


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
does not lower the accepted revision. This is not durable restart protection.
The core does not authenticate requests, persist state/reports or schedule retries. Its backoff
helper returns 5, 10, 20, 40, 80, 160 and then 300 seconds for failure counts
starting at zero, without jitter or Retry-After handling. A matching initial read
is sufficient for `verified`; a successful write requires a matching second read.
Choosing `use-permanent-password` selects a verification mode only; it does not
provision or change a password.
