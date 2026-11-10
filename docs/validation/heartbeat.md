# No-policy heartbeat validation

## Environment

The service test uses `RustDeskAgent-Validation`, Windows 11 Pro 10.0.26200,
with zero network adapters. Setup runs as `RDAGENT-LAB\LabAdmin`; the agent
runs as LocalService with its service SID. Official RustDesk is version 1.4.7.
Dummy enrollment talks only to trusted `https://localhost:8444`, forwarded to
the Console fork on guest loopback. Automatic writes are disabled.

The agent and Console changes are reviewed in
[agent #23](https://github.com/enkhjile/rustdesk-agent/pull/23) and
[Console #5](https://github.com/enkhjile/rustdesk-console/pull/5).

## Completed checks

| Behavior | Result |
| --- | --- |
| No assigned policy | GET 204 produces a persisted heartbeat with null desired/applied revisions, empty results and detected ID/version. Console stores the report and successful-contact time while its revision remains 0. |
| Lost acknowledgement and restart | Console accepted heartbeat sequence 1; the proxy dropped its acknowledgement. The protected outbox retained it. After both service and Console restarts, the exact same body was accepted again with 204. |
| Repeated heartbeat | Sequence advanced to 2 without fabricating an accepted revision. |
| First assignment | Desired revision 1 produced `unsupported` outcomes with null applied revision. |
| Policy removal | Heartbeat sequence 4 resumed with null desired/applied revisions and empty results. The agent's accepted revision and Console's revision counter remained 1. |
| Reassignment | The same enrollment received revision 2 and reported sequence 5, with `unsupported` outcomes and null applied revision. |
| Revocation | The next authenticated poll returned 401. Console's stored sequence and last-contact value did not advance. |
| Read-only operation | Before/after detection output matched exactly. Automatic writes stayed disabled; ambiguous settings defaults were not treated as verified values. |
| Device-page display | A separate temporary localhost Console with dummy data showed “Heartbeat received — no policy assigned,” metadata, sequence and last contact. Native RustDesk offline state stayed separate. Agent API requests returned 200; the only browser resource errors were missing favicons. |

The completed VM run restored and removed its checkpoint and stopped
with zero network adapters. The temporary host Console and browser were stopped.

## Automated checks

```powershell
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
cargo test --locked --test contract_backend
cargo build --locked --target x86_64-pc-windows-msvc
# Console repository
cargo test --locked -p rustdesk-console features::agent
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets
cargo build --locked -p rustdesk-console
# Console web directory
pnpm test
pnpm run build
```

Agent formatting, Clippy, Windows build and focused suites passed. The full host
library run passed 29 tests; three protected-storage tests require administrator
access. All compiled default-feature agent test executables passed in the VM,
including those storage checks. Console's 12 agent tests and 22 web tests passed,
as did its web/backend builds and Clippy (existing warnings). The full Windows
Console server suite passed 233 tests and failed two outside this change:
MMDB directory sync denied access, and recording-path comparison expected Unix
separators. Hosted Linux CI runs the full workspace suite.

Tests cover strict nullable fields, malformed heartbeat rejection, database
reopen, idempotence, sequence conflicts/exhaustion, revocation, assignment
transitions, watermark retention, failed durable saves and pending report recovery.

## Limits

No live RustDesk-health or settings-application verification is claimed. Upgrade
Console before agents. This test does not validate production networking,
MySQL/PostgreSQL, power-loss recovery or downgrade compatibility.
