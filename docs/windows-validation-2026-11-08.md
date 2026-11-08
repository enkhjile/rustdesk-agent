# Windows validation attempt — 2026-11-08 (Asia/Tokyo)

Tested commit: `0534dcab782307bddf86f41a51937577c9ea3124`, branch
`feat/durable-state-reporting` (PR #5). This is a build/test record, **not a
completed Windows service canary**. No application code was changed.

## Host and scope

Commands run from `C:\Users\user\rustdesk-agent` in PowerShell:

```powershell
whoami
whoami /groups
Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,BuildNumber
Get-CimInstance Win32_ComputerSystem | Format-List Manufacturer,Model
Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion' |
    Select-Object ProductName,DisplayVersion,CurrentBuild,UBR
Get-Service RustDeskAgent -ErrorAction SilentlyContinue
```

Account: `DESKTOP-OGKKQK4\user`, medium integrity outside the tool sandbox;
Administrators membership is deny-only, not elevated. OS: Microsoft Windows 11
Pro, version `10.0.26200`, display version `25H2`, build `26200.9711`.
CIM reports **Dell Inc. / OptiPlex 7000**, not a virtual-machine model. No
`RustDeskAgent` service was returned. Disposable VM status was not established,
so the requested builds/tests-only fallback was used. No services, enrollment,
state stores, trust roots or credentials were provisioned; no reboot or RustDesk
probe was performed. Production was not contacted.

Initial sandboxed Git/toolchain operations were denied filesystem access and
CIM queries were denied. Repeating outside the tool sandbox allowed those
operations; that did not elevate the Windows account.

## Commands and results

The missing pinned toolchain was installed without changing `rust-toolchain.toml`:

```powershell
git fetch origin feat/durable-state-reporting
rustup toolchain install 1.99.1 --profile minimal --component rustfmt --component clippy
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked --target x86_64-pc-windows-msvc
cargo test --locked --test contract_backend --test enrollment --test reconcile
cargo test --locked --bin rustdesk-agent
cargo test --locked --doc
rustc -Vv
Get-FileHash target/x86_64-pc-windows-msvc/debug/rustdesk-agent.exe -Algorithm SHA256
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
server was deployed. See [contract testing](contract-testing.md).

## Remaining gates on an isolated disposable VM

### VM setup follow-up (same day)

The user subsequently authorized creation of an isolated VM. Hyper-V is installed;
the host has approximately 32 GiB RAM and had 171 GB free on C:. The existing ISO
`C:\Users\user\Downloads\Win11_25H2_Japanese_x64.iso` contains Windows 11 Pro at
image index 3. An elevated setup applied that image to a new 64 GiB dynamic VHDX
and created `RustDeskAgent-Validation`: generation 2, 8 GiB RAM, four virtual CPUs,
vTPM, and **no network adapter**. No existing VM was replaced.

Setup is incomplete. Host-side `bcdboot G:\Windows /s F: /f UEFI /c /v`, targeting
only the mounted lab disk, failed with exit 183 / `c0000035` while opening the
lab BCD hive as `BCD00000000`. The host BCD was not repaired or unloaded. A separate
2 GiB Windows PE repair disk was prepared to perform boot setup inside the guest.
Mounting its copied `boot.wim` for modification failed because the copy retained
the ISO's read-only attribute. The resume script now clears that attribute, but
Windows canceled the subsequent UAC launch before this correction could run.

The VM has not booted and no guest service test has run. Host-local scripts and
logs are retained under `target\validation\`; they are ignored build/lab artifacts,
not tested repository tooling. `repair-in-vm.ps1` is the pending elevated resume
step. The draft `validate-guest.ps1` sequences guest tests only after a successful
boot. The host ISO and lab repair disk may remain mounted from the interrupted
setup. Next work must inspect their current state before resuming; do not recreate
or overwrite the existing lab disks. Windows UAC approval is still required.

All of the following remain **unexecuted**, including previously reported hosted
storage tests as local elevated validation:

- Run the complete pinned-toolchain suite elevated, then enroll with dummy
  credentials and initialize state under an administrator. Verify ordinary-user
  initialization is refused and repeated initialization cannot reset history.
- Use an isolated HTTPS endpoint with certificate validation intact and record
  its setup/cleanup. Keep it available across the client reboot; the current
  in-process fixture does not provide this lifecycle.
- Record the explicit service account and token/service SID. Verify agent-SID
  enrollment read and state replacement; deny ordinary users and an unrelated
  service, including a service sharing the account. Attempt state-file DACL
  changes after the agent creates replacement files, not only after admin setup.
- Exercise SCM start, stop during GET and POST, restart and reboot; record SCM
  exit status, timestamps, pending report bytes, revision and sequence before/after.
  Inject report failures and check identical retries, acknowledgement persistence
  and retained ordering. Observe absence of UAC prompts after setup.
- Preserve exact commands, Windows build, account, executable hash and results.
  Re-run the failing storage tests; do not count compilation or synthetic SIDs
  as service isolation evidence. RustDesk writes remain disabled.
