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

## Remaining isolated-VM checks

The prepared runner has **not executed**. It checkpoints both existing disposable
VMs, connects them through a private Hyper-V switch without an uplink, uses dummy
credentials and a trusted local HTTPS certificate, and restores/stops both guests.
No VM result or restoration result is claimed for this change yet.

Pending: install the downloaded package; run the agent as LocalService against
Console on the other VM; observe real setting changes; verify observe-only
upgrade behavior, report retry, policy removal/reassignment and reboot retention.
The standard-package GUI/connection comparison also remains pending. Earlier lab
executor records are supporting evidence, not substitutes for these checks.

## Password investigation

Official RustDesk 1.4.7's
[`--password` implementation](https://github.com/rustdesk/rustdesk/blob/1.4.7/src/core_main.rs)
requires an installed client and administrator/root privileges, accepts the new
password as a command-line argument, and prints a diagnostic when those checks
fail. That path provides no stdin alternative; exit status alone is insufficient.

The [daemon IPC implementation](https://github.com/rustdesk/rustdesk/blob/1.4.7/src/ipc.rs)
supports an acknowledgement for permanent-password changes. However, Windows
[main-channel authentication](https://github.com/rustdesk/rustdesk/blob/1.4.7/src/ipc/auth.rs)
also checks the peer executable against RustDesk's executable. Calling that
private channel directly from the agent is not an established supported path.

The prepared dummy-only runtime probe compares the official CLI under LocalService
and an administrator. It has not run. New/old password authentication, persistence
and process/log exposure still require VM evidence. No password API, secret
delivery, elevated helper or permanent-password handling was added to the product.
The remaining design decision is a supported privileged execution mechanism and
acceptable secret input/exposure, not whether official RustDesk has a password command.
