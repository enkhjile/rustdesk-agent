# RustDesk 1.4.7 getter diagnosis — 2026-11-08

Adding a disconnected network adapter resolved the blank ID in the isolated VM.
It did not resolve the blank settings getters. Settings writes remain disabled;
blank output is still unverified, not an observed default.

## Controlled comparison

This continues the [installed-binary validation](rustdesk-1.4.7-validation.md)
using the same binary hashes, dummy accounts and Windows 11 Pro 25H2 guest
(build 26200.6584). The host is Windows 11 Pro 25H2 build 26200.9711.
RustDesk's service was running. The agent and local HTTPS endpoint were stopped
for these raw getter probes.

At 21:19 JST, the harness queried the installed executable as elevated
`RDAGENT-LAB\LabAdmin` and through a temporary service as
`NT AUTHORITY\LocalService`, before and after a guest shutdown/start:

```powershell
& 'C:\Program Files\RustDesk\rustdesk.exe' --version
& 'C:\Program Files\RustDesk\rustdesk.exe' --get-id
& 'C:\Program Files\RustDesk\rustdesk.exe' --option approve-mode
& 'C:\Program Files\RustDesk\rustdesk.exe' --option verification-method
```

The existing `RustDeskReadProbe.exe` harness invoked these arguments directly
with redirected stdout/stderr and a three-second timeout. The host experiment
used:

```powershell
Stop-VM RustDeskAgent-Validation
Add-VMNetworkAdapter -VMName RustDeskAgent-Validation -Name Disconnected-MAC-Probe
Get-VMNetworkAdapter -VMName RustDeskAgent-Validation |
    Select-Object Name, SwitchName, MacAddress
Start-VM RustDeskAgent-Validation
```

No virtual switch was assigned. The guest reported MAC `00:15:5D:00:94:01`,
adapter status 7 (media disconnected). No production endpoint was reachable.

| Query | No adapter, both accounts | Disconnected adapter, both accounts |
| --- | --- | --- |
| `--version` | `1.4.7` | `1.4.7` |
| `--get-id` | Blank | `486577153` |
| `--option approve-mode` | Blank | Blank |
| `--option verification-method` | Blank | Blank |

Every command exited 0 with empty stderr and no timeout. Before the adapter was
added, the RustDesk `--service` process ran in session 0 and `--server` in
session 1; both `\\.\pipe\RustDesk\query_service` and
`\\.\pipe\RustDesk\query` existed. Pipe presence alone does not prove that a
particular getter successfully used IPC.

## Source explanation and limits

The exact 1.4.7 dependency revision
[generates an initial ID from a MAC address](https://github.com/rustdesk/hbb_common/blob/df6badca5bf81b4e9836256cf8e31c993ad70dd1/src/config.rs#L1024).
The observed ID matches the last four MAC bytes masked with `0x1fffffff`.
The controlled result supports missing MAC availability as the cause of the
original blank ID. It does not establish server registration or reachability.

The [CLI option getter](https://github.com/rustdesk/rustdesk/blob/1.4.7/src/core_main.rs#L523)
prints an empty string when a key is absent. Its
[IPC implementation](https://github.com/rustdesk/rustdesk/blob/1.4.7/src/ipc.rs#L1721)
can fall back to cached configuration. Thus neither exit 0 nor blank output
distinguishes a live default from a failed IPC read.

The source maps empty or unknown verification-method to both password types in
the [GUI model](https://github.com/rustdesk/rustdesk/blob/1.4.7/flutter/lib/models/server_model.dart#L71).
The standard-client [approve-mode default](https://github.com/rustdesk/rustdesk/blob/1.4.7/flutter/lib/common.dart#L3907)
is empty; the [settings control](https://github.com/rustdesk/rustdesk/blob/1.4.7/flutter/lib/desktop/pages/desktop_setting_page.dart#L1199)
labels that choice as accepting sessions via both password and click.
These are source-derived interpretations, not yet GUI observations or permission
to normalize blank CLI output into verified settings.

GUI comparison remains incomplete: VMConnect displayed the guest sign-in screen,
but computer-use input did not reach the elevated console. No guest interactive
sign-in or GUI settings observation was completed. The final harness phase named
`after-gui` is only a repeated CLI probe; its name is not evidence of GUI testing.
No option setter, direct
TOML edit, permanent-password command or custom IPC client was used.
Raw evidence and the local harness are under ignored `target/validation` and
guest `C:\Lab`; no deployment credentials belong in this document.

The final CLI repetition returned the same ID and blank settings under both
accounts. Cleanup deleted the temporary probe service, stopped RustDesk and the
agent/endpoint, shut down the VM and removed the disconnected test adapter.
The runner completed at 21:24:08 JST and recorded the VM as `Off`.
No agent implementation failure was demonstrated, so this follow-up changes
documentation only. `git diff --check` passed; no new Rust build or unit-test
run is claimed for this documentation change.
