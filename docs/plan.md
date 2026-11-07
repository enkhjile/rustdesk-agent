# Windows RustDesk agent: implementation plan

Scope: Windows x64 companion service, no server or RustDesk changes. This workspace is a project mirror, not the kiosk repository; keep work isolated here. Existing `pk-libs-rs/crates/installer-tools-cli/src/rustdesk_setup.rs` embeds RustDesk 1.4.7 and invokes `--option` and `--password` during elevated setup, but checks exit code only. It is an installer, not a polling service.

1. Inspect the exact installed console release and RustDesk client; document evidence and endpoint contract. **Partial:** console v0.2.40 source was not accessible via available public sources; no claim about its API can be made.
2. Implement and test a strict, allowlisted reconciliation core; default to no changes without an explicitly verified platform adapter.
3. Implement Windows SCM lifecycle and installation with least-privileged service identity demonstrated on a disposable machine. Build stable UUID enrollment, ACL-protected per-device credentials (DPAPI plus restrictive ACL), authenticated HTTPS with certificate validation, bounded response size/timeouts and no redirects; persist revisions atomically. Do not ship until verified.
4. Implement RustDesk adapter only after testing the exact signed binary, version, CLI read/write and service context. Never treat exit code alone as proof. Run the canary test below.
5. Build a management backend or local-only stub implementing the proposed contract; authenticate each device, bind identity server-side, authorize every request, store monotonic immutable revisions and idempotent reports, audit access and allow credential rotation/revocation. No deployment is included.

## Implementation and validation status

Implemented: one library crate with desired-state parsing, per-setting
reconciliation, report serialization and a capped backoff helper. Eight tests
cover basic rejection, successful reconciliation, read/write failures, mismatched
readback, invalid directly constructed state and backoff bounds. Drift across
repeated runs, mixed per-setting results, failure specifically on the post-write
read, and strict null/duplicate handling still need dedicated tests. See the
[contract limits](contract.md#current-implementation-limits).

Repository tooling is configured: pinned Rust, Windows CI, cargo-deny policy,
Renovate, local hooks and opt-in semantic-release. The local cargo-deny audit passed on 2026-11-07.
No remote CI or Windows service/UAC canary has been completed. The remaining gates below are
planned validation, not a record of passing tests.

## Design and alternatives

Use one-way desired state with per-setting read/apply/readback. A standalone service can be deployed and rolled back independently of the kiosk UI. Embedding the loop into setup-tools would couple background operation to an interactive installer. Direct TOML edits and a RustDesk fork are excluded. The first deliverable is the pure reconciliation core; no executable agent, production backend or usable credentials are provided yet. No permanent password is accepted: RustDesk 1.4.7 CLI uses a process argument and a secure delivery/handling path is not established.

Revision numbers are monotonically increasing per device. Reject stale revisions and reject reused revision with a different payload in the service/backend. Reconcile periodically even at the same revision to detect drift; report applied revision only when every requested value has been observed after application. Partial changes remain partial and are retried; no automatic rollback is implied. The core deliberately has no shell or generic command interface. The platform must authenticate the request before parsing state, enforce HTTPS, and bind UUID to the credential rather than trusting a caller-provided ID.

## Validation and rollout gates

- Unit: strict schema/enum rejection, per-setting results, drift, partial failure, readback failures and bounded backoff.
- Disposable Windows x64 machine with the exact 1.4.7 MSI and then each intended version: compare `--version`, `--get-id`, `--option key` and write/read for each allowed value; deliberately disable settings, stop RustDesk service and test invalid versions. Observe exit code, stdout/stderr, IPC context, empty/default value behavior and timeout. Confirm actual unattended connection behavior, reboot persistence and GUI consistency. Do not interpret blank readback as success. Determine whether a less-privileged account works; use LocalSystem only if needed. Record exact account, binary hash, Windows version and commands. Test startup, clean stop, restart, offline retry, protected files, credential revocation, no interactive UAC prompts after installation, and non-admin tampering.
- End-to-end canary: isolated TLS management stub with a test credential, one disposable client, revision changes and drift, rejection of unknown fields and stale/equivocating revisions, transient network/report failures, restarts and credential revocation. Confirm status against the actual connection experience. Canary only after evidence and explicit deployment authorization.
- Rollout: install manually in a test environment with elevated approval, test least privilege, expand to a small approved cohort only after canary. Monitor partial/failed outcomes and connectivity. Roll back by issuing a **new higher revision** with previously recorded settings; independently stop/disable and uninstall the agent. Stopping the agent does not restore prior RustDesk settings. Maintain out-of-band kiosk access. No production deployment is authorized here.

## Evidence

RustDesk 1.4.7 source implements `--get-id`, `--option key` read, and `--option key value` write via IPC and checks installation/elevation; it prints errors and returns without necessarily setting a failing exit code: https://github.com/rustdesk/rustdesk/blob/1.4.7/src/core_main.rs. Source inspection does not prove Windows service behavior. Current console README describes commands and management features, but is not evidence for v0.2.40: https://github.com/dockers-x/rustdesk-console. Exact tag/release API inspection remains a prerequisite; proposed `/v1/agent` routes are new, not existing console APIs.
