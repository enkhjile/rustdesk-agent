# Setter child cleanup after agent termination

## Environment and reproduction

Disposable `RDAGENT-LAB`, Windows 11 Pro 25H2 build 26200.6584, no network
adapters, official installed RustDesk 1.4.7. The official client hash is recorded
in the [setter runtime record](2026-11-09-setter-modes-and-timeout.md). The agent
service and local HTTPS fixture stayed stopped; automatic writes remained disabled.

As LabAdmin, the harness launched the actual agent command:

```powershell
& C:\Lab\rustdesk-agent-locked.exe --set-option 'C:\Program Files\RustDesk\rustdesk.exe' approve-mode password
```

It suspended the direct RustDesk child and confirmed that its command line was
`--option approve-mode password`, rather than the preceding version query. It
then forcibly terminated the agent and checked the tracked child after 500 ms.
On merged `3fce667` (the binary hash in the [serialization record](2026-11-09-serialized-setters.md)),
the suspended child survived. The harness explicitly terminated it before restoring settings.

## Fix and results

The explicit CLI now joins a process-lifetime Windows Job Object before spawning
RustDesk. The job handle is unnamed, non-inheritable and retained until OS process
teardown; closing it terminates member descendants. This avoids relying on Rust
destructors during forced termination or assigning children after they start.
Containment setup failure prevents the command from launching RustDesk.

The fixed Windows x64 debug binary, built with Rust 1.99.1, had SHA256
`F1429DC4B93B2CBFB446C317503DFEFB6595006035F42BC07F175AF5CE27ADFC`.
It was tested as `C:\Lab\rustdesk-agent-job.exe` with the same arguments.

- After forced agent termination, the captured setter child was absent at the
  500 ms check. The killed agent produced no success report.
- Suspending the actual setter without killing the agent still produced exit 1,
  `TimedOut`, and no surviving direct child after the approximately three-second
  deadline.
- A regression test reproduced surviving children before implementation and
  passed afterward. It checks both children and grandchildren after normal and
  forced parent exit, and preserves normal exit status.
- Twenty overlapping administrator/LocalService rounds retained both requested
  settings: all 40 account readbacks matched. Sequential preservation and the
  bounded busy-lock rejection also passed with containment enabled.
- The full compiled unit suites passed in the elevated guest: 24 library tests
  and 8 binary tests. On the non-elevated host, the two administrator-only
  protected-store tests failed as expected; all remaining tests, including
  integration tests, passed when those two were excluded.

Build checks used the pinned toolchain:

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked --target x86_64-pc-windows-msvc
```

Formatting, Clippy and the Windows x64 build passed. The test limitation above
applies to the host test command. Both accounts' original empty option values
were restored with RustDesk running and stopped, then checked after restarting
RustDesk. Temporary helper services were removed, the original automatic startup
mode was restored, and the network-isolated VM was shut down.

## Scope

Forced termination of the real setter was tested as administrator. Successful
LocalService setters exercise containment under that account, but forced death
of a LocalService setter was not separately tested. Existing RustDesk service/GUI
processes are outside the job. Read-only detection and polling behavior are unchanged.
The test does not establish rollback, atomic lock-release/child-exit ordering,
live configuration provenance or compatibility with every parent-job restriction.
No permanent password was handled or provisioned.

Raw evidence is retained under ignored `target/validation/abrupt/` and
`target/validation/containment-retest/`: child outcome logs, test-binary results,
per-phase readbacks and restoration/final-state records.
