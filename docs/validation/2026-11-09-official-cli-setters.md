# Official CLI setter behavior

> Historical validation record: results and pending checks describe the tested
> revision and date, not current project status. See the [current plan](../plan.md)
> and [validation index](README.md) for subsequent results.

The agent's explicit `--set-option` command works with official RustDesk 1.4.7
under both an administrator and LocalService in the isolated VM. Automatic
service writes remain disabled. Matching CLI readback is not independent proof
of live connection behavior.

## Environment and commands

- Host: Windows 11 Pro 25H2, build 26200.9711; pinned Rust 1.99.1.
- Guest: `RustDeskAgent-Validation` / `RDAGENT-LAB`, Windows 11 Pro 25H2,
  build 26200.6584, no network adapters. Dummy accounts only.
- Accounts: `RDAGENT-LAB\LabAdmin` through PowerShell Direct, and
  `NT AUTHORITY\LOCAL SERVICE` through temporary `RustDeskLabSettingsProbe`.
- Official executable: `C:\Program Files\RustDesk\rustdesk.exe`, SHA256
  `7A43F30721826E6D6B11686A47B3F6D672DC58C1BDB2B62076D704E8BA5235C0`.
- Agent: `C:\Lab\rustdesk-agent-setter.exe`, Windows x64 debug build, SHA256
  `BCCB78A0FF822EE2B87DBB2D739FEB2F2EEDE36E33B63CB8D15A1F941FF16988`.
  Tested setter implementation: commit `7f497af` (PR #12).
  The existing agent service executable was not replaced.

Exact setter commands, first two as LabAdmin, last two as LocalService:

```powershell
& C:\Lab\rustdesk-agent-setter.exe --set-option 'C:\Program Files\RustDesk\rustdesk.exe' approve-mode password
& C:\Lab\rustdesk-agent-setter.exe --set-option 'C:\Program Files\RustDesk\rustdesk.exe' verification-method use-temporary-password
& C:\Lab\rustdesk-agent-setter.exe --set-option 'C:\Program Files\RustDesk\rustdesk.exe' approve-mode click
& C:\Lab\rustdesk-agent-setter.exe --set-option 'C:\Program Files\RustDesk\rustdesk.exe' verification-method use-both-passwords
```

Each invocation runs `--version`, then `--option <key> <value>`, then
`--option <key>` against the official executable. The test helper additionally
runs both official getters independently after writes and under the other
account. It uses `UseShellExecute=false`, not interactive elevation.

## Results

| Check | Result |
| --- | --- |
| Baseline under both accounts | Both raw option values empty |
| Administrator writes; LocalService reads | `password`, `use-temporary-password` |
| LocalService writes; administrator reads | `click`, `use-both-passwords` |
| `Restart-Service RustDesk`, both accounts read | Both values retained |
| Host `Stop-VM` / `Start-VM`, both accounts read | Both values retained |
| Restore both accounts; final LocalService read | Both raw option values empty again |
| Cleanup | Temporary service removed, VM `Off` |

All four agent invocations exited zero, with empty stderr,
`command_accepted=true`, `readback_matches=true`, and
`live_state_verified=false`. No guest UAC interaction was needed. Host elevation
was required for Hyper-V. Agent and local HTTPS fixture were stopped during
the test. No server contact, TOML editing, password provisioning or client patch.

A separate direct-official-CLI test also showed click approval in the locked GUI.
Verification-method controls were hidden in that mode and were **not** visually
verified. This GUI observation was not part of the actual agent setter run above.

## Cleanup and evidence

Both original raw option values were restored and read back under both accounts.
Restoration used the official CLI with an explicit empty final argument:
`--option approve-mode ""` and `--option verification-method ""`, then getters.
The agent deliberately rejects empty values. The temporary service was removed,
agent/endpoint stopped and VM shut down.

Local evidence remains under ignored `target/validation/`:
`run-agent-setters.ps1`, `AgentSettingsProbe.cs`, `agent-setters-v1.host.log`,
`settings-agent-v1-*.txt`, and `agent-setters-v1-final-state.txt`.

## Automated checks and remaining gates

`cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, and
`cargo build --locked --target x86_64-pc-windows-msvc` passed. Six new setter tests
cover the allowlist, exact commands, version gate, exit-zero diagnostics,
failed/mismatching readback, and relative-path rejection. Existing subprocess
tests exercise real-child deadlines and output limits.

`cargo test --locked --no-fail-fast` in the sandbox encountered blocked loopback
sockets and two administrator-required storage tests. Outside the socket sandbox,
`cargo test --locked -- --skip protected_store_is_create_once_and_keeps_identity
--skip protected_state_replaces_reopens_and_excludes_other_writers` passed all 57
remaining tests. Subsequently, [hosted Windows CI for commit 7f497af](https://github.com/enkhjile/rustdesk-agent/actions/runs/42553423154)
passed all 59 tests, including both administrator-only storage tests, plus
formatting, Clippy, build and dependency checks.

Not covered: password-click and permanent-only mode runtime writes, independent
live verification of verification-method, actual incoming connection behavior,
and cancellation during a real setter. Binary provenance enforcement and an
authorized rollout remain deployment gates. These limitations do not require a
custom RustDesk fork; they prevent claiming live verification or enabling
automatic management application in this change.
