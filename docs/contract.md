# Proposed management contract (not implemented)

Separate service or minimal extension, not an existing console API. TLS with normal hostname/certificate validation is mandatory. Each bearer credential identifies exactly one provisioned UUID; never authorize based on a body or path UUID alone. Credentials must not appear in URLs, process arguments, reports or logs. Backend retains a hash, supports rotation/revocation and audit; enrollment and authorization UX remain to be designed. Limit requests/responses and enforce read/write deadlines. No redirects. Fixed origin is provisioned locally by an administrator. These routes and JSON are proposals, not claims about v0.2.40.

`GET /v1/agent/desired` with `Authorization: Bearer <device credential>` returns HTTP 200, content-type JSON:

```json
{"schema_version":1,"revision":7,"settings":{"approve_mode":"password","verification_method":"use-permanent-password"}}
```

The proposed protocol requires all objects to reject unknown or duplicate fields. Revision is a positive u64, monotonic per device, and denotes immutable content. Settings contains at least one of the two optional keys. Allowed values: approve_mode = `password`, `click`, `password-click`; verification_method = `use-temporary-password`, `use-permanent-password`, `use-both-passwords`. An omitted setting is unmanaged, not reset. Explicit null setting values are not part of the proposed protocol. No secret or command is accepted. A 204 means no assigned state; 401/403 means stop applying and alert/retry at bounded intervals; 429/5xx/network errors retry with bounded backoff. Use a cap and jitter in the service, and honor a bounded Retry-After. No lower or equivocal revision may be applied, including after restart.

`POST /v1/agent/status` with the same authorization contains:

```json
{"schema_version":1,"agent_uuid":"a49f96c4-968b-4e7a-a3be-a5452385584d","rustdesk_id":"123456789","rustdesk_version":"1.4.7","desired_revision":7,"applied_revision":null,"results":{"approve_mode":"verified","verification_method":"mismatch"}}
```

`rustdesk_id` and `rustdesk_version` may be null when unavailable. `applied_revision` is null unless all settings in the current desired revision were independently read back as requested; `results` values are `verified`, `read_failed`, `write_failed`, `mismatch`, or (future adapter) `unsupported`. The backend validates UUID against credential and accepts duplicate reports idempotently. A report is retried after failure; subsequent polls also reconcile actual state, including unchanged revisions. The implementation here serializes only the reconciliation portion; identity, durable state, HTTP and report transport are not implemented.


## Current implementation limits

`parse_desired` checks schema version 1, a positive revision, at least one
non-null setting, allowed enum values and unknown fields. Callers should use this
function rather than deserialize `DesiredState` directly, which skips the semantic
checks. `reconcile` repeats the schema/revision/nonempty checks before any I/O.

The current optional-field deserializer accepts null as an omitted setting.
Duplicate keys are rejected, including when the earlier value is null. Strict
null rejection and dedicated duplicate-key regression tests are still needed
before using this parser for the proposed endpoint. There are no machine-readable JSON Schema files
yet; the examples above describe the intended contract.

The core does not track previous revisions, reject stale or changed same-revision
payloads, authenticate requests, persist reports or schedule retries. Its backoff
helper returns 5, 10, 20, 40, 80, 160 and then 300 seconds for failure counts
starting at zero, without jitter or Retry-After handling. A matching initial read
is sufficient for `verified`; a successful write requires a matching second read.
Choosing `use-permanent-password` selects a verification mode only; it does not
provision or change a password.
