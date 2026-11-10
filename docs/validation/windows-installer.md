# Windows installer validation

## Environment and scope

- Disposable `RustDeskAgent-Validation` VM, Windows 11 Pro build 26200,
  with no network adapters. Administrator: `RDAGENT-LAB\LabAdmin`.
- Official RustDesk 1.4.7. Installed agent service: LocalService with its
  service SID; automatic settings application remains disabled.
- Rust 1.99.1, Windows x64 MSVC. Two debug artifacts built from the same source
  are prepared for replacement testing; this does not establish cross-version
  state-schema compatibility.
- The prepared enrolled-service test uses dummy credentials and a guest-local
  HTTPS endpoint backed by the Console fork. No production endpoint is involved.

## Completed

```powershell
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --locked
cargo build --locked --target x86_64-pc-windows-msvc
cargo test --locked --no-run --message-format=json
```

Formatting, Clippy and both builds passed. The compiled default-feature test
executables passed in the VM as administrator, using `--test-threads=1`.
On the non-elevated host, `cargo test --locked` passed 26 library tests and failed
the three protected-storage tests requiring administrator access. Those three
passed in the VM. The installed agent build uses a static C runtime; import
inspection found no dynamic Visual C++ runtime dependency.

The first lifecycle run installed a stopped LocalService service and exercised
validation refusals, but stopped after the unsafe-ACL test restored permissions.
The installer compared DACL bookkeeping flags as well as permissions. The
comparison now checks ACEs exactly and inheritance protection independently;
the lifecycle test retains this regression case. Rejection tests also check the
expected error, so an unrelated failure cannot count as successful refusal.

The VM checkpoint was restored, its temporary checkpoint removed, and the VM
stopped with zero network adapters. No RustDesk settings were changed.

## Remaining runtime gate

The corrected lifecycle and enrolled-service scenarios still require a completed
VM run. They are not established by compilation or the first partial run:

```powershell
.\tests\installer.ps1 -AgentPath C:\Staging\rustdesk-agent.exe `
  -RustDeskPath 'C:\Program Files\RustDesk\rustdesk.exe'
```

The checked-in test requires a fresh disposable administrator environment. It
covers stopped install/upgrade/uninstall/reinstall and refusal of invalid hashes,
invalid payloads, weakened permissions and unexpected files. Hosted CI runs this
test with a never-executed placeholder RustDesk executable.

The separate enrolled-service VM scenario must confirm that stopped upgrades
preserve enrollment/state bytes and ACLs; running upgrades resume reports with
the same identity/revision and increasing sequence; uninstall retains stores;
and reinstall resumes without reenrollment or state initialization. It must
also compare RustDesk detection before/after and verify `unsupported` outcomes
with null applied revision. Until that run passes, installer runtime validation
is incomplete. Signing and production rollout remain separate gates.
