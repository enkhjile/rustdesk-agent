# Windows RustDesk agent: implementation plan

Scope: Windows x64 companion service, no production server changes or RustDesk modifications. This workspace is a project mirror, not the kiosk repository; keep work isolated here. Existing `pk-libs-rs/crates/installer-tools-cli/src/rustdesk_setup.rs` embeds RustDesk 1.4.7 and invokes `--option` and `--password` during elevated setup, but checks exit code only. It is an installer, not a polling service.

## Ordered milestones

1. **Evidence and capability probe (partial).** Published console v0.2.40 API
   inspected; see [API evidence](api-evidence.md). On an authorized disposable
   Windows x64 machine, probe the exact RustDesk binary under candidate service
   accounts, including read/write/readback for both settings. Record binary hash,
   version, account and results. Exit: a supported version/context is demonstrated,
   or writes remain disabled. Do not assume the installed production version.
2. **Portable reconciliation (in-memory slice implemented).** The revision guard
   rejects stale revisions and changed content at the same revision, comparing
   parsed settings rather than raw JSON. Retain the strict allowlist and readback tests.
   Exit: tests prove invalid state causes no settings I/O, while unchanged
   revisions still repair drift.
3. **Contract test backend (implemented; local and hosted Windows tests).** A loopback HTTPS fixture
   in `tests/support/` uses Rust dev-dependencies and JSON Schema validation.
   Six passing local tests in `tests/contract_backend.rs` cover the routes with disposable
   credentials, device isolation, immutable revisions, report ordering and TLS.
   Exit: contract/schema tests pass. A production backend or Console extension
   remains separate work; the existing Console API does not supply these routes.
4. **Windows service and enrollment (implemented; service runtime validation pending).**
   The [read-only service](windows-service.md) handles SCM lifecycle and local
   diagnostic snapshots. [Local enrollment](enrollment.md), stable UUID storage and
   machine-DPAPI protection with service-SID ACLs are implemented. PR #3 passed hosted
   Windows DPAPI/storage tests; actual service-SID access, reboot/UAC validation
   and server registration remain pending.
   Complete administrator-only local provisioning. Generate
   the UUID once per enrolled machine; do not bake identity or credentials into
   cloned images. Protect credentials with DPAPI and restrictive ACLs for the
   service SID and administrators. Machine scope avoids administrator/service
   profile coupling; ACL enforcement and service-SID access still need Windows
   runtime validation. Exit: Windows x64 build plus service
   restart, non-admin access and no-post-install-UAC tests pass. No RustDesk writes
   in this milestone; LocalSystem only with evidence that it is required.
5. **Authenticated polling and durable state (read-only slice implemented; Windows runtime gates pending).**
   Fixed-origin HTTPS polling now uses enrollment, certificate validation, no redirects,
   bounded bodies, deadlines, stop-aware service integration, and capped retry with jitter.
   Accepted state, observation sequence and latest pending report are now persisted
   together. Status delivery reports `unsupported` with null applied revision until
   live verification is enabled. See [state initialization and recovery](state.md).
   Exit: isolated tests cover restart/crash boundaries, authentication failures,
   stale state, report retries, malformed responses and network outages.
6. **Verified adapter and canary (pending; needs 1 and 5).** Invoke only fixed
   RustDesk CLI arguments through a trusted absolute executable path, without a
   shell. Enforce process deadlines and recheck version/capabilities after upgrades;
   unsupported versions disable writes. Exit: per-setting outcomes, actual
   connection behavior, reboot persistence and service/UAC tests pass on the
   disposable client before any separately authorized rollout.

Keep one crate and add each module only when its milestone begins. The core revision guard remains in-memory; management now restores and persists
its watermark and latest report in milestone 5. Read-only polling must not be mistaken for applied-state reporting. The Windows probe is the prerequisite
for validating an account and enabling writes. Service skeleton development can
proceed from source evidence with runtime behavior explicitly unverified. No production changes,
credential changes or deployment are part of these implementation milestones.

## Implementation and validation status

Implemented: one library crate with desired-state parsing, per-setting
reconciliation, report serialization and a capped backoff helper. A Windows service
skeleton and read-only detector now provide local snapshots; see [usage and limits](windows-service.md). Eighteen reconciliation tests
cover rejection (including explicit null and duplicate keys), successful
reconciliation, drift at the same revision, partial results, read/write failures,
failed post-write readback, mismatched readback, invalid directly constructed
state and backoff bounds, plus revision rejection without I/O, semantic equality
and retention of accepted revisions after application failure. See the [contract limits](contract.md#current-implementation-limits).

Repository tooling is configured: pinned Rust, Windows CI, cargo-deny policy,
Renovate, local hooks and automatic semantic-release. The local cargo-deny audit passed on 2026-11-07.
PRs #1–#4 passed hosted Windows x64 checks and dependency checks before merging;
`v1.0.0-alpha.4` is published. PR #3 includes passing DPAPI/storage tests.
An [isolated Windows VM run](windows-vm-validation.md) passed SCM/HTTPS, service-SID,
retry, cancellation, ordinary-user token access denial and unattended reboot checks.
The ordinary-user CLI initialization launch and
the installed-RustDesk canary remain pending. The remaining gates below are
planned validation, not a record of passing tests.

## Design and alternatives

Use one-way desired state with per-setting read/apply/readback. A standalone service can be deployed and rolled back independently of the kiosk UI. Embedding the loop into setup-tools would couple background operation to an interactive installer. Direct TOML edits and a RustDesk fork are excluded. The first deliverable is the pure reconciliation core; a development executable now provides read-only detection and an SCM skeleton. No production backend or enrolled credentials are provided. No permanent password is accepted: RustDesk 1.4.7 CLI uses a process argument and a secure delivery/handling path is not established.

Revision numbers are monotonically increasing per device. Reject stale revisions and reject reused revision with a different payload in the service/backend. Reconcile periodically even at the same revision to detect drift; report applied revision only when every requested value has been observed after application. Partial changes remain partial and are retried; no automatic rollback is implied. The core deliberately has no shell or generic command interface. The platform must authenticate the request before parsing state, enforce HTTPS, and bind UUID to the credential rather than trusting a caller-provided ID.

## State and failure rules

- Track **accepted state** separately from **applied revision**. Atomically persist
  the highest accepted revision and its parsed settings before any write. A failed
  write does not lower that watermark. Missing state on an enrolled installation
  or corrupt state disables writes until administrative recovery; never silently
  reset the revision or generate a replacement identity.
- Only reconcile after a successful authenticated 200 response that passes all
  validation. On 204, authentication failure, invalid responses or outages, leave
  current RustDesk settings alone and do not apply cached state. Retain the
  revision watermark across 204 responses. Authentication failure is visible
  locally and retries are bounded; no interactive prompts.
- Keep one reconciliation cycle active at a time. Persist the latest report;
  retries must not overwrite a newer report with an older one. A later observation
  can supersede an unsent report for the same revision. Backend receipt time alone
  is insufficient for ordering; use the contract's per-device observation sequence. Agent-side sequence storage is implemented with the pending report.
- A successful initial read already verifies an unchanged setting. After a write,
  require a matching readback. Settings are not transactional: partial success
  remains partial, and no automatic rollback is implied.
- No enrollment HTTP API or admin UI is needed for the test MVP: provision the
  UUID/credential mapping locally with test data. Production enrollment, rotation
  and revocation operations must be defined before rollout. Secrets never appear
  in command arguments, logs or status. Use local diagnostic snapshots for unassigned devices and polling errors in this
  slice; network heartbeat/error reporting remains separate contract work; the current status schema
  only represents a reconciliation attempt with a desired revision.

## Validation and rollout gates

- Unit: strict schema/enum rejection, per-setting results, drift, partial failure, readback failures and bounded backoff.
- Disposable Windows x64 machine with the exact 1.4.7 MSI and then each intended version: compare `--version`, `--get-id`, `--option key` and write/read for each allowed value; deliberately disable settings, stop RustDesk service and test invalid versions. Observe exit code, stdout/stderr, IPC context, empty/default value behavior and timeout. Confirm actual unattended connection behavior, reboot persistence and GUI consistency. Do not interpret blank readback as success. Determine whether a less-privileged account works; use LocalSystem only if needed. Record exact account, binary hash, Windows version and commands. Test startup, clean stop, restart, offline retry, protected files, credential revocation, no interactive UAC prompts after installation, and non-admin tampering.
- End-to-end canary: isolated TLS management stub with a test credential, one disposable client, revision changes and drift, rejection of unknown fields and stale/equivocating revisions, transient network/report failures, restarts and credential revocation. Confirm status against the actual connection experience. Canary only after evidence and explicit deployment authorization.
- Before distributing executable assets, include dependency license notices and select the application distribution license.
- Rollout: install manually in a test environment with elevated approval, test least privilege, expand to a small approved cohort only after canary. Monitor partial/failed outcomes and connectivity. Roll back by issuing a **new higher revision** with previously recorded settings; independently stop/disable and uninstall the agent. Stopping the agent does not restore prior RustDesk settings. Maintain out-of-band kiosk access. No production deployment is authorized here.

## Evidence

RustDesk 1.4.7 source implements `--get-id`, `--option key` read, and `--option key value` write via IPC and checks installation/elevation; it prints errors and returns without necessarily setting a failing exit code: https://github.com/rustdesk/rustdesk/blob/1.4.7/src/core_main.rs. Source inspection does not prove Windows service behavior. The exact console v0.2.40 published OpenAPI was inspected; see [API evidence](api-evidence.md). Proposed `/v1/agent` routes are new, not existing console APIs.

## Folder architecture

Keep a single crate. Existing `src/lib.rs` owns the portable allowlist and
reconciliation; `tests/reconcile.rs` exercises behavior. `docs/schemas/` defines
the proposed wire contract. Add modules only as their milestone is implemented:
`src/management.rs` now handles HTTPS polling, durable revision acceptance and read-only status delivery, `src/windows/` now holds SCM lifecycle, `src/probe.rs` the bounded read-only
CLI runner, and `src/main.rs` the Windows entry point. Credential storage now lives in `src/windows/credentials.rs` with portable
validation in `src/enrollment.rs`; read-only management polling is integrated. Keep the test backend separate from the installed agent.
`src/state.rs` defines the bounded snapshot and report, while `src/windows/state_store.rs` owns protected file replacement and writer locking. No extra workspace crates are needed.
