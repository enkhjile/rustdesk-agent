# Concurrent setting changes

## Environment and method

The existing disposable `RDAGENT-LAB` VM ran Windows 11 Pro 25H2 build
26200.6584 with no network adapters. Official RustDesk 1.4.7 and the actual agent
executable had the hashes recorded in the [setter runtime record](2026-11-09-setter-modes-and-timeout.md).
RustDesk was running; the agent service and local HTTPS fixture were stopped.
Automatic writes remained disabled. Accounts were administrator
`RDAGENT-LAB\LabAdmin` and `NT AUTHORITY\LocalService` via a temporary service.

Both original raw values were empty. Before each case, the actual agent set
`approve-mode=password` and `verification-method=use-temporary-password`.
The tested changes were:

```powershell
& C:\Lab\rustdesk-agent-setter.exe --set-option 'C:\Program Files\RustDesk\rustdesk.exe' approve-mode click
& C:\Lab\rustdesk-agent-setter.exe --set-option 'C:\Program Files\RustDesk\rustdesk.exe' verification-method use-both-passwords
```

First, each account ran those changes sequentially. Getters checked both the
changed setting and the untouched setting after each command. Both accounts
then read the final pair.

Next, 20 rounds launched the two agent commands behind a shared start barrier.
For ten rounds LabAdmin changed approval and LocalService changed verification;
for ten rounds their roles were reversed. Measured agent-call intervals
overlapped in every round, by approximately 240–276 ms. This confirms overlapping
calls, not a specific internal RustDesk IPC interleaving. After both completed,
fresh official getters under both accounts read both settings.

## Results

Sequential changes passed under both accounts: changing approval preserved
verification, and changing verification preserved approval. Both accounts read
the expected final pair.

| Concurrent outcome | Rounds | Final readback under both accounts |
| --- | --- | --- |
| Both requested changes retained | 16/20 | `click`, `use-both-passwords` |
| Verification change lost; approval retained | 3/20 | `click`, `use-temporary-password` |
| Approval change lost; verification retained | 1/20 | `password`, `use-both-passwords` |

In all four lost-update cases, the LocalService invocation returned exit 1,
`command_accepted=true`, `readback_matches=false`, and the original value for its
requested setting. Its diagnostic was `Settings command accepted, but CLI
readback did not match`. The other invocation returned exit 0 with matching
readback. All results retained `live_state_verified=false`.

Thus changes to different allowed keys can interfere when run concurrently.
The agent detected each mismatch observed in this run; it did not silently
report those four commands as successful. The counts describe this sample,
not a general failure probability. No claim is made about RustDesk's internal
cause, arbitrary options, simultaneous GUI changes or live connection behavior.

## Operational consequence and cleanup

Run explicit setting commands sequentially. Future automatic application must
serialize its own writers, handle interference from external writers and retain
partial/mismatch outcomes. Serializing only this agent cannot exclude RustDesk
GUI or other external changes. Matching CLI readback still cannot establish
live application, as the [unavailable-process test](2026-11-09-unavailable-processes.md)
demonstrated. This test does not enable automatic writes or add automatic retries.

Official setters restored the original values under both accounts with RustDesk
running and stopped. Final getters after restarting RustDesk were empty under
both accounts. Temporary services were removed, the original startup mode
restored, agent/endpoint stopped, and Hyper-V confirmed the VM Off. No permanent
password was provisioned or handled.

Raw evidence remains in ignored `target/validation/concurrent/`: per-phase
`concurrent-*.txt`, `summary.json`, `host.log`, `restore.txt` and `final-state.txt`.
