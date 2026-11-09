# Setter modes, deadline and connection behavior

> Dated test evidence, not deployment approval. See the [current plan](../plan.md)
> for remaining work.

## Environment

- Agent implementation: merged commit `511098f`, Rust 1.99.1, Windows x64 debug build.
  SHA256: `34477A38B5BF5030E221318A211614C71FF397E16985E5BEA8B4EB3B440A83BE`.
- Official installed RustDesk 1.4.7, SHA256:
  `7A43F30721826E6D6B11686A47B3F6D672DC58C1BDB2B62076D704E8BA5235C0`.
- Disposable Windows 11 Pro 25H2 guest, build 26200.6584, no network adapters.
  Accounts: administrator `RDAGENT-LAB\LabAdmin` and `NT AUTHORITY\LocalService`.
  The agent service and local HTTPS fixture were stopped; dummy credentials only.

## Setter and GUI results

Both accounts invoked these commands using the actual agent executable:

```powershell
& C:\Lab\rustdesk-agent-setter.exe --set-option 'C:\Program Files\RustDesk\rustdesk.exe' approve-mode password-click
& C:\Lab\rustdesk-agent-setter.exe --set-option 'C:\Program Files\RustDesk\rustdesk.exe' verification-method use-permanent-password
```

Both commands exited zero with empty stderr, `command_accepted=true`,
`readback_matches=true` and `live_state_verified=false`. Direct official getters
under the other account matched. Baselines were restored between account tests,
so both accounts exercised changes from empty raw values.

After LocalService applied the values, the locked RustDesk GUI showed:

| Field | Observed Japanese label | Meaning |
| --- | --- | --- |
| Approval dropdown | 両方の方法でセッションを承認 | Accept by password or click |
| Selected verification radio | 固定パスワードを使用する | Use permanent password |

This independently confirms the displayed selections. It does not prove a
permanent password exists or that an incoming connection succeeds. None was provisioned.

Repeating the same requested values with the RustDesk service stopped also
returned matching readback. This was not a changed-value or all-processes-absent
test; another RustDesk process or cached configuration may explain the result.

## Actual setter deadline

The test suspended the actual direct RustDesk child executing
`--option approve-mode password`. The agent exited 1 with empty stdout and
`Settings command failed: TimedOut` after about three seconds (2,986 ms measured
from the suspension observation). The direct child was absent after the deadline.

This verifies timeout-driven cancellation and failure reporting for a real setter.
It does not test abrupt agent termination, every IPC hang, descendant cleanup or
rollback. A failed command may have changed a setting before cancellation.

## Cleanup and limits

Both original raw values were restored using official CLI setters with empty
final arguments, then read back under both accounts. Temporary services were
removed, the agent/endpoint stopped, and Hyper-V confirmed the VM was off.
No agent implementation failure was demonstrated; automatic service writes remain disabled.

Disabled-settings policy, concurrent changes and all-processes-unavailable
behavior remain separate tests. Raw evidence is retained
under ignored `target/validation/`: `agent-setters-v2.host.log`,
`settings-agent-v2-*.txt`, `agent-setters-v3.host.log`, `setter-timeout-v3.log`,
and the corresponding final-state files. GUI results were observed in the VM console.

## Private two-VM connection tests

The same target and a fresh Windows 11 Pro 25H2 peer (build 26200.6584) ran
official RustDesk 1.4.7. Each had one adapter on a private Hyper-V switch, with
addresses `192.0.2.10/24` and `192.0.2.11/24`, no default routes and no host or
Internet uplink. The peer did not receive a cloned agent identity or enrollment.
Both interactive users were lab administrators; the target agent service stayed
stopped. These connection results do not establish application from polling.

Direct-IP access was temporarily enabled through the official CLI's
`--option direct-server Y`; a target firewall rule allowed TCP 21118 only from
the peer. The peer launched official RustDesk with `--connect 192.0.2.10`.
The actual agent's `--set-option` command applied each tested setting pair.

| Approval / verification | Observed connection behavior |
| --- | --- |
| `password-click` / `use-temporary-password` | Target displayed an incoming request. After the user clicked Accept, the peer displayed the target desktop. No password was entered. |
| `password` / `use-temporary-password` | Fresh connection displayed a password prompt. After the user entered the target's temporary password without saving it, the peer displayed the target desktop. |
| `password` / `use-permanent-password` | In a repeat run, the user first authenticated successfully in temporary-only mode, then retried that same temporary password after the agent selected permanent-only mode. The fresh connection showed “Password is incorrect” and did not open the desktop. |

These are independent connection observations, beyond matching CLI readback.
They do not cover every allowed setting combination or permanent-password
authentication. No permanent password was provisioned.

After testing, the original raw values of both settings and `direct-server` were
restored with official CLI setters and checked by getters. The cleanup runner
removed the lab firewall rule, both temporary network adapters and the private
switch. Hyper-V reported both VMs off.

Raw connection evidence remains in ignored `target/validation/`:
`private-connection.host.log`, `password-connection.host.log` and
`private-settings.log`, plus the corresponding logs under `password-retry-2/`;
desktop and dialog results were observed in the consoles. The user entered the
passwords; their values are not recorded here. The rejection demonstrates the
tested transition, not successful permanent-password authentication or every
password-lifetime condition.

## Repository checks

`cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings` and
`cargo build --locked` passed with the pinned toolchain. Outside the network
sandbox, `cargo test --locked`
passed 22 library tests but failed the two protected-store tests because the host
process was not an elevated administrator. Running the suite with just those two
tests excluded passed, including integration tests. This does not replace the
administrator-only checks or the earlier isolated-VM ACL evidence.
