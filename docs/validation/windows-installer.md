# Windows installer validation

## Environment and scope

- Disposable `RustDeskAgent-Validation` VM, Windows 11 Pro build 26200,
  with no network adapters. Administrator: `RDAGENT-LAB\LabAdmin`.
- Official RustDesk 1.4.7. Installed agent service: LocalService with its
  service SID; automatic settings application remains disabled.
- Rust 1.99.1, Windows x64 MSVC. Two debug artifacts built from the same source
  were used for replacement testing; this does not establish cross-version
  state-schema compatibility.
- The enrolled-service test used dummy credentials and a guest-local
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

The lifecycle test verifies exact ACEs and inheritance protection independently
of DACL bookkeeping flags, including after permissions are restored. Rejection
tests check the expected error, so unrelated failures cannot count as successful
refusals.

The VM checkpoint was restored, its temporary checkpoint removed, and the VM
stopped with zero network adapters. No RustDesk settings were changed.

[Hosted Windows Server 2022 CI](https://github.com/enkhjile/rustdesk-agent/actions/runs/42739073979)
passed the corrected installer lifecycle test, including the restored-ACL
regression, stopped upgrade, uninstall/reinstall and all refusal cases. Both
default and `lab-apply` Rust tests, formatting, Clippy, build and dependency checks
passed. The installer did not start the service during these hosted checks.

## Enrolled-service VM results

The lifecycle checks passed in the isolated VM with official RustDesk:

```powershell
.\tests\installer.ps1 -AgentPath C:\Staging\rustdesk-agent.exe `
  -RustDeskPath 'C:\Program Files\RustDesk\rustdesk.exe'
```

The checked-in test requires a fresh disposable administrator environment. It
covers stopped install/upgrade/uninstall/reinstall and refusal of invalid hashes,
invalid payloads, weakened permissions and unexpected files. Hosted CI runs this
test with a never-executed placeholder RustDesk executable.

The enrolled-service scenario also passed:

| Behavior | Observed result |
| --- | --- |
| Fresh install | LocalService, automatic start configuration, service SID enabled, service initially stopped; no automatic-write flag. |
| Stopped upgrade | Executable hash changed to the second artifact; enrollment/state file hashes and ACLs stayed identical; service stayed stopped. |
| Running upgrade | Service restarted and Console received a higher observation sequence with the same UUID and desired revision 1. |
| Uninstall | Service and executable removed; enrollment/state file hashes and ACLs stayed identical. |
| Reinstall | Existing stores reused without reenrollment or initialization; Console received sequence 4 under the same identity. |
| Running uninstall | Service stopped and removal completed. |
| Writes disabled | Both results remained `unsupported`, with null applied revision. RustDesk detection before/after was identical, including ambiguous-default settings observations; this is not live setting verification. |
| Cleanup | Checkpoint restored and removed; VM off, zero network adapters. |

The scenario used these installer operations, with dummy enrollment, Console
binding and one administrator `--init-state` after the first install (hashes
computed from the staged builds):

```powershell
$installer = 'C:\Lab\Installer\scripts\install.ps1'
$old = 'C:\Lab\Installer\agent-old.exe'
$new = 'C:\Lab\Installer\agent-new.exe'
$oldHash = (Get-FileHash $old).Hash
$newHash = (Get-FileHash $new).Hash
& $installer -Action Install -SourcePath $old -Sha256 $oldHash
# Enroll, bind, initialize, start, wait for report, then stop.
& $installer -Action Upgrade -SourcePath $new -Sha256 $newHash
Start-Service RustDeskAgent
# Wait for a new report before exercising running replacement.
& $installer -Action Upgrade -SourcePath $old -Sha256 $oldHash
Stop-Service RustDeskAgent
& $installer -Action Uninstall
& $installer -Action Install -SourcePath $new -Sha256 $newHash
Start-Service RustDeskAgent
# Wait for sequence advancement before running uninstall.
& $installer -Action Uninstall
```

## Limits

Replacement used distinct artifacts of the same source, not different released
versions or state schemas. No schema migration, power-loss recovery during
installation, or installer rollback was tested. Signing, distribution licensing
and production rollout remain separate gates. Automatic writes stay disabled.
