# RustDesk 1.4.7 getter and GUI comparison

> Historical validation record: results and pending checks describe the tested
> revision and date, not current project status. See the [current plan](../plan.md)
> and [validation index](README.md) for subsequent results.

Adding a disconnected network adapter resolved the blank ID in the isolated VM.
It did not resolve the blank settings getters. Settings writes remain disabled;
blank output is still unverified, not an observed default.

## Controlled comparison

This continues the [installed-binary validation](2026-11-08-rustdesk-1.4.7-getters.md)
using the same binary hashes, dummy accounts and Windows 11 Pro 25H2 guest
(build 26200.6584). The host is Windows 11 Pro 25H2 build 26200.9711.
RustDesk's service was running. The agent and local HTTPS endpoint were stopped
for these raw getter probes.

The harness queried the installed executable as elevated
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
At this stage these were source-derived interpretations, not GUI observations or permission
to normalize blank CLI output into verified settings.

## GUI comparison

The same isolated guest and installed 1.4.7 executable were used. This run had
**no network adapters**; the previously generated ID survived adapter removal
and guest restart. The GUI was inspected while signed in as `RDAGENT-LAB\LabAdmin`.
Security settings remained
locked throughout observation; no option was changed or password provisioned.

The main window displayed ID `486 577 153` and a connecting-to-network status.
Settings → Security → Password displayed:

| Field | Observed Japanese GUI label | Meaning |
| --- | --- | --- |
| Approval dropdown | 両方の方法でセッションを承認 | Accept sessions via both methods |
| Selected verification radio | 両方のパスワードを使用する | Use both passwords |

These selections match the source-derived defaults described above. They do not
prove that a permanent password exists or that a remote connection would work.

With the GUI open, the same four fixed CLI probes were
repeated as elevated LabAdmin through PowerShell Direct and as LocalService
through the temporary `RustDeskLabReadProbe` service. Both returned version
`1.4.7`, ID `486577153`, and newline-only output for both option getters. All
exited 0, with empty stderr and no timeout. The transcript
recorded LabAdmin active in console session 1, the RustDesk service in session 0,
and its server, tray and GUI processes in session 1.

Opening the GUI did not make either setting getter return an explicit value.
The observed GUI defaults are consistent with blank CLI output, but the CLI's
cache fallback still prevents distinguishing a live default from a failed IPC
read. This completes the default-state GUI comparison only; it does not verify
non-default values, setting changes, live CLI readback or actual connections.
This run issued no setters and does not justify normalizing blank output into
verified settings. See the subsequent [setter record](2026-11-09-official-cli-setters.md)
for non-default CLI readback; independent live verification remains separate.

## Cleanup and evidence

Evidence: host `target/validation/gui-comparison.host.log` and
`gui-comparison-cli.log`, plus the inspected VM console screens. Screenshots
containing the transient password are not committed. Cleanup deleted the probe
service, stopped the agent, RustDesk service and endpoint, and shut down the VM.
The temporary disconnected adapter was removed. The final VM state was `Off`
with no adapters. No agent code, RustDesk settings or passwords were changed.
