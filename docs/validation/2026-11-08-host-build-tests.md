# Windows host build and test results

> Historical validation record: results and pending checks describe the tested
> revision and date, not current project status. See the [current plan](../plan.md)
> and [validation index](README.md) for subsequent results.

Tested commit: `0534dcab782307bddf86f41a51937577c9ea3124`, branch
`feat/durable-state-reporting` (PR #5). This is a build/test record, **not a
completed Windows service canary**. No application code was changed.

## Host and scope

Account: `DESKTOP-OGKKQK4\user`, medium integrity outside the tool sandbox;
Administrators membership is deny-only, not elevated. OS: Microsoft Windows 11
Pro, version `10.0.26200`, display version `25H2`, build `26200.9711`.
CIM reports **Dell Inc. / OptiPlex 7000**, not a virtual-machine model. No
`RustDeskAgent` service was returned. Disposable VM status was not established,
so the requested builds/tests-only fallback was used. No services, enrollment,
state stores, trust roots or credentials were provisioned; no reboot or RustDesk
probe was performed. Production was not contacted.

## Commands and results

Checks used the pinned Rust 1.99.1 toolchain:

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked --target x86_64-pc-windows-msvc
cargo test --locked --test contract_backend --test enrollment --test reconcile
cargo test --locked --bin rustdesk-agent
cargo test --locked --doc
```

- Formatting and Clippy passed with Rust 1.99.1.
- Windows x64 MSVC debug build passed.
- Full test command failed: 16 library tests passed and two failed immediately
  at `require_admin().unwrap()` with `PermissionDenied` / `run enrollment as an
  elevated administrator`: `credentials::tests::protected_store_is_create_once_and_keeps_identity`
  and `state_store::tests::protected_state_replaces_reopens_and_excludes_other_writers`.
  Their protected storage operations were not exercised. This is a runner
  prerequisite failure; the checks were not removed, ignored or weakened.
- Separately executed integration targets passed: contract backend 6,
  enrollment 5, reconciliation 18.
- Binary target: 1 test passed; documentation target: 0 tests, successful.
  Across executed targets: 46 passed, 2 failed at the elevation prerequisite.
- Passing library tests include DPAPI round-trip/tamper rejection, serialized
  revision/sequence retention, report retry/restart, acknowledgement-save failure,
  terminal report rejection, HTTP cancellation and request deadlines. These are
  in-process tests, not SCM/reboot or service-token evidence.

Compiler: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, LLVM 23.1.1.
Built debug executable SHA-256:
`6E11A0A2FEF767B2DF3E74A572238565743C5F1BBAB3C54341D56D50075E7A2C`.

## Backend inspection

`tests/support/mod.rs` implements an in-process Rust fixture. `Fixture::start`
binds `127.0.0.1:0`, generates an ephemeral certificate/key, and adds the certificate
explicitly to its test client. Assignments, fault injection and reports are held
in memory; dropping the fixture stops it. The service executable cannot simply
use that client or its ephemeral trust configuration. No standalone management
server was deployed. See [contract testing](../contract-testing.md).

## Limits and follow-up

This run covered builds and in-process tests only. Administrator-only storage,
SCM lifecycle, service-token isolation and reboot behavior were not exercised
on the physical host. Those checks subsequently passed in the
[isolated VM run](2026-11-08-service-lifecycle.md).

No host services, credentials or RustDesk settings were changed. Raw logs remain
under ignored `target/validation/`; VM setup troubleshooting is not part of the
agent's behavior or these test results.
