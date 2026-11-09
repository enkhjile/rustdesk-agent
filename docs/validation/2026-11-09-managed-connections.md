# Managed revisions and live connection behavior

## Environment and method

Disposable Windows VMs `RustDeskAgent-Validation` (`RDAGENT-LAB`) and
`RustDeskAgent-Peer` (`RDAGENT-PEER`), using the existing Windows 11 Pro 25H2 lab.
LabAdmin controlled the test; the target's actual RustDeskAgent service ran as
`NT AUTHORITY\LocalService` with its service SID. A private Hyper-V switch joined
`192.0.2.10` and `192.0.2.11`, without a host/Internet uplink or guest default route.
Direct-IP access was temporarily enabled through the official RustDesk CLI; a
temporary target firewall rule allowed TCP 21118 only from the peer.

Official RustDesk 1.4.7 on both guests had SHA256
`7A43F30721826E6D6B11686A47B3F6D672DC58C1BDB2B62076D704E8BA5235C0`.
Agent commit `49dcf5a` was built with pinned Rust 1.99.1:

```powershell
cargo build --locked --target x86_64-pc-windows-msvc --features lab-apply
```

The tested agent SHA256 was
`4AB97B0DB3262F0B175E40A75FA968E5F9E0CD283DEBC7CF12649A44E868BB8A`.
The temporary service command was:

```text
C:\Lab\rustdesk-agent-lab.exe --service "C:\Program Files\RustDesk\rustdesk.exe" C:\ProgramData\RustDeskAgent\managed-connection-2.json --lab-apply
```

The local HTTPS fixture at `https://localhost:8443` supplied fresh desired
revisions using the existing dummy enrollment and normally validated TLS.
Each phase started the service, waited for its application/report cycle, then
stopped it before the interactive connection check. The harness did not directly
set either managed setting. The peer launched official RustDesk with:

```powershell
& 'C:\Program Files\RustDesk\rustdesk.exe' --connect 192.0.2.10
```

## Results

| Desired settings | Independent connection observation |
| --- | --- |
| `click` / `use-temporary-password` | Peer waited for acceptance. After the user accepted on the target, the peer displayed its desktop without password entry. |
| `password` / `use-temporary-password` | Fresh connection requested a password. The user entered the target's temporary password without saving it; the peer displayed the target desktop. |
| `password` / `use-permanent-password` | After a successful temporary-password connection, a higher managed revision selected permanent-only mode. Reusing that same temporary password on a fresh connection showed “Password is incorrect”; the desktop did not open. |

The click phase used revision 18/sequence 22. The completed temporary/permanent
comparison used revisions 21 and 22/sequences 25 and 26. Every phase delivered
both results as `unverified_cli_match` with null applied revision. Administrator
CLI readback matched both requested values, and pending reports were drained.
No product failure was demonstrated.

Regression coverage exercises successive settings revisions through the HTTPS
fixture, exact callback values, report sequence and result keys, null applied
revision, and leaving an omitted setting unmanaged. A temporary mutation that
reused the old approval value failed this test; restored source passed.
Formatting, all-feature Clippy, Windows x64 build and non-administrator tests
passed. Full host `cargo test --locked` still requires elevation for the two
protected-store tests; this run does not replace those checks.

## Limits and restoration

These user-assisted connection observations validate the tested managed
transitions, not an automatic live verifier. They do not establish successful
permanent-password authentication, every setting combination, or every temporary
password lifetime condition. No permanent RustDesk password was provisioned.
The agent still cannot promote a CLI match to `verified` or an applied revision.
The lab feature stays default-off and production application stays disabled.

Both accounts' original managed values were restored with RustDesk running and
stopped, then read back after restart. The original `direct-server` value,
agent service arguments, RustDesk startup mode and endpoint task state were
restored. Temporary helper services, endpoint task, firewall rule, network
adapters and private switch were removed; both VMs were shut down. Durable
enrollment and revision/sequence history were retained.

Raw reports, assertions and the runner remain under ignored
`target/validation/managed-connection-2/`; console outcomes were visually inspected.
Passwords are not included in this record.
