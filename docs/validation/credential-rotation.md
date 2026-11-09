# Credential rotation and recovery

Scope: agent PR #21 with Console PR #2, using the normal service configuration
without `--lab-apply`. Production writes remain disabled.

Environment: disposable `RustDeskAgent-Validation`, Windows 11 Pro
10.0.26200, official RustDesk 1.4.7. Administrator setup used `LabAdmin`;
`RustDeskAgent` ran as `NT AUTHORITY\LocalService` with its service SID.
Console listened on guest loopback HTTP, behind a trusted local HTTPS endpoint
at `https://localhost:8444`. Only dummy lab credentials were used. The runner
refused network adapters attached to non-private switches.

| Behavior | Result |
| --- | --- |
| Replace while enrolled service holds the state writer lock | Rejected; encrypted enrollment unchanged. |
| Rotate in Console before updating the local credential | Old token returned 401; agent retained its revision watermark. |
| Lose a replacement and issue another | Previous replacement rejected; newest credential usable. |
| Supply a short invalid credential locally | Rejected; existing enrollment unchanged. |
| Elevated local replacement | UUID and origin retained; durable state file remained byte-for-byte identical. |
| Restart Console and the Windows service | Old tokens still rejected; service resumed reports with the same UUID/revision and increasing sequence. |
| Revoke, rotate and replace | Device recovered without reenrollment or state initialization. |
| Protected staged-file recovery and replacement failure | Windows tests passed; failed replacement retained the original encrypted blob, retry succeeded, unexpected policy/identity rejected. |
| Ordinary-user `--replace-credential` rejection | Passed: actual CLI exited 1 with `PermissionDenied`; enrollment/state bytes and permissions unchanged. |

The final report retained desired revision 1 and advanced observation sequence
to 3. Both setting results remained `unsupported` with null applied revision.
Before/after detector results matched (`ambiguous_default` for both settings);
this is not independent live-state verification. The pretest VM checkpoint was
restored and removed, and the VM was shut down.

The ordinary-user check ran the same agent executable from a temporary Windows
service under the dummy local account `RDAGENT-LAB\RotationUser`. The wrapper
verified its account SID and `admin=False` before launching the actual CLI with
a syntactically valid dummy credential on redirected stdin. The CLI emitted no
stdout and returned `run enrollment as an elevated administrator` on stderr,
with exit code 1. Both protected files' SHA-256 hashes and security descriptors
matched before and after. This establishes rejection under an ordinary user's
service-logon token; an interactive desktop session was not needed for the test.
Checkpoint restoration removed the temporary service, account and test files.
Cleanup confirmed the VM off, zero network adapters and zero probe checkpoints.

Commands exercised:

```text
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
cargo test --locked --features lab-apply
cargo build --locked --target x86_64-pc-windows-msvc
rustdesk-agent.exe --enroll https://localhost:8444
rustdesk-agent.exe --init-state
rustdesk-agent.exe --replace-credential
rustdesk-agent.exe --service "C:\Program Files\RustDesk\rustdesk.exe" C:\ProgramData\RustDeskAgent\credential-rotation.json
POST /api/admin/agent/:id/rotate
POST /api/admin/agent/:id/revoke
```

Credentials entered through redirected stdin, never command arguments. Agent
tests used pinned Rust 1.99.1. The built test executables passed in the elevated
guest with `--test-threads=1`; hosted Windows CI also passed both feature sets.
Console tests covered administrator authorization, old-token rejection on both
routes, history preservation, inactive devices and concurrent report/rotation.

Tested executable SHA-256:

- Agent: `000C74C0858FB63C39BD70C119D6D9406F782FD59028B8FAA9AB0A67409D242A`
- Console: `82A073804CAA5073107DB3EC639FE078C7AAB2252971EC9FEC92F9F687E8BE82`

Remaining limits: physical power loss during replacement and recovery of
destroyed enrollment/state are not established by
this run. Missing/corrupt local stores remain rejected, not repaired or reset.
