# Setters with RustDesk processes unavailable

## Environment and method

Disposable `RDAGENT-LAB`, Windows 11 Pro 25H2 build 26200.6584, with no network
adapters. The actual agent executable and official RustDesk 1.4.7 had the same
SHA256 hashes recorded in the [preceding runtime tests](2026-11-09-setter-modes-and-timeout.md).
The merged documentation changes did not change those binaries. The agent service
and local HTTPS fixture stayed stopped; automatic writes remained disabled.

Tests ran separately as administrator `RDAGENT-LAB\LabAdmin` and
`NT AUTHORITY\LocalService` (a temporary service). Both original raw settings
were empty under both accounts. Before each account's test, the harness:

1. Set the RustDesk service startup type to Disabled and stopped it.
2. Stopped every remaining `rustdesk.exe`, after checking its executable path
   matched the installed official client.
3. Waited and confirmed zero RustDesk processes, then invoked changed values:

```powershell
& C:\Lab\rustdesk-agent-setter.exe --set-option 'C:\Program Files\RustDesk\rustdesk.exe' approve-mode password
& C:\Lab\rustdesk-agent-setter.exe --set-option 'C:\Program Files\RustDesk\rustdesk.exe' verification-method use-temporary-password
```

Process counts were zero before each setter and after each setter/getter returned.
The commands necessarily launched short-lived CLI processes; this was absence of
the background RustDesk processes, not absence of the CLI executable. It was not
continuous process tracing. With startup still disabled, both accounts queried:

```powershell
& 'C:\Program Files\RustDesk\rustdesk.exe' --option approve-mode
& 'C:\Program Files\RustDesk\rustdesk.exe' --option verification-method
```

The harness then enabled manual service startup, started RustDesk and repeated
both accounts' getters. Original values were restored before testing the second
account, including each account's offline CLI state.

## Results

Both setters, under both accounts, exited zero with empty stderr and reported
`command_accepted=true`, `readback_matches=true`, `status=unverified_cli_value`
and `live_state_verified=false`.

| Account issuing the offline setters | Same-account offline getters | Other-account offline getters | Both accounts after service restart |
| --- | --- | --- | --- |
| LabAdmin | Requested values | Original empty values | Original empty values |
| LocalService | Requested values | Original empty values | Requested values |

Thus successful command acceptance and matching readback can occur without a
running RustDesk service/server. Offline observations differed by account, and
the LocalService changes became visible across accounts after service startup.
This is consistent with CLI cache fallback; the test did not inspect TOML or
establish the internal persistence mechanism. Post-start getters alone still do
not prove live connection behavior.

No agent implementation failure was demonstrated: its output already marks
these results unverified. A future automatic application loop must not equate
exit zero or matching CLI values with `verified`, nor assume an offline command
cannot affect a later service start.

## Restoration and limits

Official CLI setters restored both original raw values under both accounts,
with RustDesk running and stopped. Final getters after another service start
were empty under both accounts. The original service startup type was restored,
temporary probe services removed, agent/endpoint stopped, and Hyper-V reported
the VM Off. No permanent password was handled or provisioned.

Raw evidence remains in ignored `target/validation/unavailable/`: `host.log`,
the per-phase `unavailable-*.txt` files, `restore.txt` and `final-state.txt`.
This run covers the two changed values above, not every value, disabled-settings
policy, concurrent changes, abrupt agent termination or live connection behavior
after an offline write.
