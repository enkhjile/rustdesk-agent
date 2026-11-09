# Serialized explicit setter validation

## Environment

Disposable `RDAGENT-LAB`, Windows 11 Pro 25H2 build 26200.6584, no network
adapters, official installed RustDesk 1.4.7. The official client hash is unchanged
from the [earlier setter tests](2026-11-09-setter-modes-and-timeout.md).
The new Windows x64 debug agent, built with pinned Rust 1.99.1, had SHA256
`D8C36BF35193C824C1979EC4BC7816A5A62C01D60B9B06BBAEE1E0E0B6689C1A`.
The agent service and HTTPS fixture stayed stopped; automatic writes remained disabled.

## Results

The [previous 20-round test](2026-11-09-concurrent-settings.md) was repeated with
the new executable at `C:\Lab\rustdesk-agent-locked.exe`. Each round began at
`password` / `use-temporary-password`, then synchronized these commands:

```powershell
& C:\Lab\rustdesk-agent-locked.exe --set-option 'C:\Program Files\RustDesk\rustdesk.exe' approve-mode click
& C:\Lab\rustdesk-agent-locked.exe --set-option 'C:\Program Files\RustDesk\rustdesk.exe' verification-method use-both-passwords
```

LabAdmin changed approval and LocalService changed verification for ten rounds;
the roles were reversed for ten. Agent-call intervals overlapped in every round;
the mutex serialized the protected operations. All 40 invocations exited zero,
and all 40 final pair checks (both accounts across 20 rounds) read both requested
values. No update was lost in this sample, versus four lost updates before the
lock. Sequential preservation checks also passed under both accounts.

With another process holding the production mutex, an actual CLI invocation
exited 1 with empty stdout and `Settings busy` after approximately 3.4 seconds
including process startup (the lock wait is limited to three seconds). Both
settings remained at their original values under both accounts.

Tests cover cross-process exclusion, timeout, release/wakeup and abandoned-owner
failure/release. The contention tests failed before the lock implementation and
passed afterward. VM test binaries passed all 24 library tests and all six
binary tests, including the administrator-only protected-store tests. On the
non-elevated host, the full test command failed only those two administrator-only
tests; the suite excluding those two passed, including integration tests.
Formatting, Clippy and the Windows x64 build passed.

## Restoration and limits

Both original raw settings were restored under both accounts with RustDesk
running and stopped. Final getters after a restart were empty under both
accounts. Temporary services were removed, the original startup mode restored,
agent/endpoint stopped, and Hyper-V confirmed the VM Off.

This validates participating explicit CLI commands, not external writers, old
agent versions, direct library calls, live connection verification or rollback.
Abandoned-owner unit coverage is not proof of child cleanup after abrupt agent
termination. No permanent password was provisioned or handled. Raw evidence is
under ignored `target/validation/serialized/`, including the per-phase results,
`busy.txt`, `summary.json`, unit-test output, restoration and final-state files.
