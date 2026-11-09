# Windows RustDesk agent: current plan

Scope: Windows x64 companion agent using official RustDesk. No custom client,
direct TOML editing, permanent-password provisioning or production deployment.
This document describes the current branch, including the explicit setter merged
in PR #12; dated results belong in the [validation archive](validation/README.md).

## Implemented and validated

| Area | Current behavior and evidence |
| --- | --- |
| Desired-state core | Strict allowlist, revision guards and per-setting reconciliation tested with an in-memory adapter; not connected to service writes. |
| Contract fixture | In-process Rust loopback HTTPS backend with schema, authentication, revision and report-ordering tests. No production server. |
| Windows service and enrollment | SCM lifecycle, machine-DPAPI enrollment and service-SID ACLs. VM tests passed service access, ordinary-user/unrelated-service denial, administrator initialization and unattended reboot. |
| Durable polling | Authenticated HTTPS, bounded retries, revision watermark, observation sequence and pending report. Real-service tests passed GET/POST cancellation, retry identity and restart/reboot retention. Reports remain `unsupported` with null applied revision. |
| Read-only detector | Bounded official CLI queries; settings are `unverified_cli_value` or `ambiguous_default`, never verified live. A real stalled getter child was terminated on service stop. |
| Explicit local setter | `--set-option` accepts only approve-mode and verification-method values for 1.4.7. Tested as administrator and LocalService, with cross-account readback and restart/reboot persistence. It is not invoked by polling. |

The [service record](validation/2026-11-08-service-lifecycle.md) contains the isolation
and lifecycle evidence. The [getter diagnosis](validation/2026-11-08-getter-diagnosis-and-gui.md)
explains blank defaults and the missing-NIC ID result. The
[setter record](validation/2026-11-09-official-cli-setters.md) records exact commands,
hashes, restored originals and passing hosted CI.

Official RustDesk supports the tested setters without modification. The upstream
CLI proposal [#11](https://github.com/enkhjile/rustdesk-agent/pull/11) is closed as
superseded by [#12](https://github.com/enkhjile/rustdesk-agent/pull/12). CLI cache
fallback still prevents treating a matching getter as independent live proof;
that limitation is not evidence that setters need a custom client.

## Next work and remaining gates

1. **Finish isolated setter behavior tests.** All six allowed values now have CLI
   runtime coverage across the two setter runs. The remaining modes
   (`password-click` and permanent-only verification) also matched the locked GUI,
   and a real stalled setter hit its deadline and was terminated; see the
   [runtime record](validation/2026-11-09-setter-modes-and-timeout.md).
   Private two-VM tests also confirmed click acceptance with `password-click`
   and temporary-password authentication with `password`, both using
   `use-temporary-password`. A repeat test rejected the previously working
   temporary password after switching to permanent-only verification.
   With background RustDesk processes absent, both accounts' setters still
   returned matching unverified readback; post-start results differed by account.
   See the [unavailable-process test](validation/2026-11-09-unavailable-processes.md).
   Other connection combinations, disabled settings, abrupt agent termination
   and whole-map/concurrent changes remain
   untested. Keep command acceptance, CLI readback, GUI state and connection
   behavior distinct.
2. **Define verified automatic application.** Establish what evidence is sufficient
   for a per-setting `verified` result before connecting the adapter to polling.
   Exercise revision changes, drift, partial outcomes and status against actual
   connection behavior in an isolated management canary. Automatic service writes
   stay disabled until this is implemented and validated.
3. **Complete deployment work.** Implement a production backend or Console
   extension, server registration, credential rotation/revocation and audit.
   Define heartbeat/error reporting without a desired revision. Validate installer
   permissions, executable provenance/signing, report-directory protection,
   report-write failure handling and physical power-loss durability. Select a
   distribution license and include dependency notices before executable release.
4. **Authorize a rollout separately.** Maintain out-of-band access and test a
   small approved cohort before expansion. No production deployment is authorized
   by the isolated VM results.

Other RustDesk versions require their own validation; exact 1.4.7 is the current
CLI gate. LocalService worked in the tested environment, so LocalSystem is not
required by those results. Production account selection remains explicit.

## Design and failure rules

Keep one Rust crate and a standalone Windows service, independent of kiosk UI or
interactive installation. The existing pk-libs-rs installer demonstrates official
CLI setters, but its exit-status-only checks are not live verification. The
[Console API evidence](api-evidence.md) distinguishes existing APIs from this
repository's [test-backend contract](contract.md).

- Keep accepted revision separate from applied revision. Persist accepted state,
  sequence and pending status before delivery; never silently reset corrupt state
  or regenerate an enrolled identity. See [state recovery](state.md).
- Authenticate and validate before any future automatic application. Do not apply
  cached state after 204, invalid responses or outages. Retain the watermark.
- The future application loop must serialize cycles, reject stale/equivocating
  revisions, repair drift at unchanged revisions, and preserve partial outcomes.
  Matching CLI cache output alone cannot satisfy its live-verification requirement.
- Settings are not transactional. No automatic rollback is implied; a failed or
  canceled explicit command may have changed a setting. A future managed rollback
  requires a new higher revision. Stopping the agent does not restore settings.
- Device credentials remain out of arguments, logs and reports. Enrollment is
  local provisioning, not server registration; do not clone enrolled identities.

Operational details belong in [Windows usage](windows-service.md),
[enrollment](enrollment.md), [state](state.md) and [contract testing](contract-testing.md).
Repository layout and build/release instructions belong in [README](../README.md).
