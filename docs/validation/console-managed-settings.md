# Normal Console-driven settings management

Changes: [agent #25](https://github.com/enkhjile/rustdesk-agent/pull/25) and
[Console #6](https://github.com/enkhjile/rustdesk-console/pull/6). Neither is merged.

## Completed checks

- Agent formatting, Clippy, Windows x64 release build and hosted Windows tests
  passed. Installer tests cover default Managed install, explicit ObserveOnly,
  preserved modes on upgrade, switching back to Managed, and uninstall/reinstall.
- The HTTPS fixture accepts managed polling at a non-loopback hostname with
  certificate validation. A failed report retains its mode and exact retry body
  across restart; later observations reflect the current mode.
- Existing application tests cover stale/conflicting revisions, omitted settings,
  failed reads/writes, partial outcomes, cancellation, durable intent and delivery
  retries. These are fixture tests, not a separate-host VM result.
- Console accepts old reports and strict optional mode metadata. Changed mode at
  the same sequence conflicts. Current policy edits appear as pending without
  changing last contact or allocating revisions on administrator status reads.
- Real Chrome against a loopback-only Console test instance displayed Managed,
  desired values and explicitly unverified CLI results. The strategy link opened
  the existing editor. Changing approval to Password only made the device panel
  show the new desired value and Pending current policy, hiding the old result.
  These reports were synthetic API fixtures; no Windows agent was attached to
  this browser test. A panel regression test also covers policy removal.

Commands: `cargo fmt --check`,
`cargo clippy --locked --all-targets --all-features -- -D warnings`,
`cargo test --locked`, and
`cargo build --locked --release --target x86_64-pc-windows-msvc --no-default-features`.
Pinned toolchain: Rust 1.99.1. The non-elevated host cannot run the three protected
storage tests; hosted Windows CI ran them successfully. Console checks use
`cargo test --locked -p rustdesk-console features::agent`, `vitest run`,
`tsc -b` and `vite build`.

[Agent CI](https://github.com/enkhjile/rustdesk-agent/actions/runs/42762920855)
passed, including installer checks. Its downloaded package has source commit
`c1143e38bc1e5f63d14850a4ec0d1b70f8d9ead7` (PR merge commit); all package
SHA-256 entries were checked. [Console CI](https://github.com/enkhjile/rustdesk-console/actions/runs/42762924219)
passed for the initial feature commit; subsequent changes require their own green run.

## Isolated-VM results

The standard CI package from [run 42764210910](https://github.com/enkhjile/rustdesk-agent/actions/runs/42764210910)
was installed on `RustDeskAgent-Validation` (`RDAGENT-LAB`). Source merge commit:
`6960e9867a759f39d62e271a4b8986269fa4df89`; executable SHA-256:
`6BDDE010651DB465EC22AE460C2CE5D7626A3F820E058A5133FDFBA8968E140E`.
Console commit `27f7614` ran on `RustDeskAgent-Peer` at
`https://192.0.2.11:8444`, with a test certificate trusted by the target VM.
The private Hyper-V network had no host/Internet uplink or default route.
Only dummy credentials were used. The guest reports Windows 10 Pro, build 26200;
the agent service ran as `NT AUTHORITY\LocalService`, setup as `LabAdmin`.

| Behavior | Result |
| --- | --- |
| Elevated agent tests | All 78 passed, including protected enrollment/state tests. |
| Installer lifecycle and modes | Managed default, explicit ObserveOnly, preserved modes on upgrades, switching back, uninstall/reinstall passed. |
| Remote Console policy | Revision 1 reported both settings as `unverified_cli_match` in managed mode. |
| Dropped acknowledgement/restart | Exactly identical sequence-1 report retried and accepted; both deliveries returned 204 at Console. |
| Observe-only upgrade | Remained observe-only after another upgrade; revision 2 reported `unsupported`, with unchanged administrator detector observations. |
| Explicit managed upgrade | The same revision 2 was reconsidered and reported `unverified_cli_match`, with enrollment preserved. |
| Policy removal | Heartbeat sequence 4 had null revision and empty results; accepted watermark stayed at revision 2. |
| Reassignment | Revision 3 produced sequence 5 with matching CLI evidence. |
| Unattended reboot | Managed mode and revision 3 survived; sequence advanced to 6 without guest login. |
| Cleanup | Both checkpoints restored; both VMs Off, zero network adapters, private test switch removed. |

Commands used the normal package's `install.ps1 -Action Install`,
`-Action Upgrade -Mode ObserveOnly`, `-Action Upgrade` and
`-Action Upgrade -Mode Managed`, supplying `-SourcePath` and its checked `-Sha256`.
Enrollment used `--enroll https://192.0.2.11:8444` with redirected stdin, followed
by administrator `--init-state`. Cycles used `Start-Service RustDeskAgent`,
`Stop-Service RustDeskAgent` and `Restart-Computer -Force`. The installer test
was `tests\installer.ps1 -AgentPath <CI executable> -RustDeskPath
'C:\Program Files\RustDesk\rustdesk.exe'`. Getter comparisons used the agent's
`--detect` command, not exit status alone.

CLI results remain unverified and `applied_revision` stayed null throughout.
The standard-package GUI/connection comparison remains pending. Earlier lab
connection tests are supporting evidence, not a new independent live-verification
result for this package. This run does not authorize production deployment.

## Password investigation

Official RustDesk 1.4.7's
[`--password` implementation](https://github.com/rustdesk/rustdesk/blob/1.4.7/src/core_main.rs)
requires an installed client and its `is_root()` check, which on Windows also
accepts an elevated token. This is not equivalent to requiring an administrator
account. It accepts the new password as a command-line argument and provides no
stdin alternative in that branch. Exit status alone is insufficient.

The [daemon IPC implementation](https://github.com/rustdesk/rustdesk/blob/1.4.7/src/ipc.rs)
supports an acknowledgement for permanent-password changes. However, Windows
[main-channel authentication](https://github.com/rustdesk/rustdesk/blob/1.4.7/src/ipc/auth.rs)
also checks the peer executable against RustDesk's executable. Calling that
private channel directly from the agent is not an established supported path.

The installed binary (SHA-256
`7A43F30721826E6D6B11686A47B3F6D672DC58C1BDB2B62076D704E8BA5235C0`)
was invoked with `--password <dummy>` under both `NT AUTHORITY\LocalService` and
`RDAGENT-LAB\LabAdmin`. Both returned exit 0, `Done!`, and empty stderr. A temporary
service using the same LocalService account as the agent supplied the first
invocation. The probe recorded only account, exit code and diagnostic categories;
the dummy value was not included in results. Its checkpoint was restored, leaving
the VM Off with zero network adapters and no retained password change.

This establishes command acceptance from the existing service account; no account
promotion or custom RustDesk build is required by that result. It does **not**
prove fresh connections accept the new password or reject the old one. Those
checks, restart persistence and process/log exposure testing remain necessary
before shipping password management. The next implementation needs protected
delivery, bounded execution and honest outcome reporting around the official CLI's
argument-based input. No password API, secret delivery, elevated helper or
permanent-password handling was added to the product in these PRs.
