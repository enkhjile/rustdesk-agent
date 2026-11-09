# Managed application crash and reboot recovery

## Environment and commands

Disposable `RustDeskAgent-Validation` (`RDAGENT-LAB`), Windows 11 Pro 25H2 build
26200.6584, with no network adapters. LabAdmin controlled the test; the actual
agent service ran as `NT AUTHORITY\LocalService` with its service SID. Existing
dummy enrollment and trusted loopback HTTPS at `https://localhost:8443` were reused.
No credential or durable-state reset was performed.

Merged `8ca9cc0` was built with Rust 1.99.1:

```powershell
cargo build --locked --target x86_64-pc-windows-msvc --features lab-apply
```

The tested agent SHA256 was
`FB5FED3ADD035C19CD73A18E8B21C6976D90344CA6E1FD55C4F5718B6A14BA16`.
Official RustDesk 1.4.7 retained SHA256
`7A43F30721826E6D6B11686A47B3F6D672DC58C1BDB2B62076D704E8BA5235C0`.
The temporary service command line was:

```text
C:\Lab\rustdesk-agent-lab.exe --service "C:\Program Files\RustDesk\rustdesk.exe" C:\ProgramData\RustDeskAgent\managed-crash.json --lab-apply
```

The harness suspended the worker's real RustDesk child, confirmed its command
line was `--option approve-mode password-click`, and called `Process.Kill()` on
the root agent service. It did not mistake the version query or lock wait for a
setter. The guest later rebooted with `shutdown.exe /r /t 2 /f`; a changed Windows
boot time and new automatic HTTP requests confirmed the reboot without guest login.

## Results

- Forced service termination removed both the worker and the suspended setter.
  The on-disk attempt remained byte-identical: revision 17, observation sequence
  20, both results `indeterminate`, and applied revision null.
- Restart with GET 204 and POST 503 retained the pending report and sequence,
  retried reporting, and left the observed approval value unchanged.
- A real guest reboot retained the same revision, sequence and pending results.
  Automatic retries before and after reboot had byte-identical report bodies.
- A subsequent POST 204 cleared the outbox without advancing sequence. Lower
  revisions and changed same-revision content were rejected after reboot.
- A fresh valid response for revision 17 resumed application. Both CLI readbacks
  matched, the new observation used sequence 21, and results remained
  `unverified_cli_match` with null applied revision. The pending report was drained.

No product failure was demonstrated. Regression coverage now also checks normal
and forced exit with nested service/worker jobs, and interrupted-report retries
across poller reconstruction followed by application only after a fresh response.
Formatting, all-feature Clippy, Windows x64 checks and the host test suite passed
apart from the two administrator-only protected-store tests. Full elevated guest
unit suites passed (28 library and 8 binary tests); host integrations and the new
regression checks passed separately.

Both accounts' original empty settings were restored while RustDesk was running
and stopped, then rechecked after restart. Service arguments, startup mode and the
original endpoint task's enabled state were restored. Temporary helper services
and the experimental endpoint task were removed, and the VM was shut down.
Revision/sequence history was preserved. The `lab-apply` feature remains default-off;
production application is disabled. No TOML editing, permanent password or custom
RustDesk client was used.

## Remaining limits

This tests one captured setter interruption followed by an orderly OS reboot,
not physical power loss or every possible write/persistence crash window. The
setter may already have changed a value before suspension; no atomicity or rollback
is implied. CLI readback can still come from cache. Independent live connection
verification remains required before production application.

Raw evidence and the runner remain under ignored `target/validation/managed-crash/`.
