# RustDesk agent repository setup status

Updated 2026-11-07 after local setup. This records the final setup decisions;
[README](../../../README.md) is the current development guide. The agent roadmap
and validation gates remain in [plan](../../plan.md).

## Completed locally

- Initialized Git on `main` and committed the setup. Published to the public `enkhjile/rustdesk-agent` repository.
- Kept one `rustdesk-agent` library package, Rust 2024 and tracked Cargo.lock.
- Pinned Rust 1.99.1 with rustfmt and Clippy.
- Added EditorConfig, text line-ending rules, and build/secret ignore rules.
- Configured Renovate with grouped Cargo/action updates and no automatic merging.
- Added cargo-deny policy and its Docker-based Linux CI job.
- Added native Windows x64 formatting, Clippy, tests and build in one CI job.
  There is no separate portable-core CI job; the core can be tested locally.
- Used `actions-rust-lang/setup-rust-toolchain` and full SHA action pins.
- Added local formatting and Conventional Commit hooks. There is no commit-message
  validation in CI. Squash feature PRs; preserve history when merging release branches.
- Configured automatic semantic-release on `main` (stable) and `alpha`
  (prerelease). GitHub releases and tags only; Cargo files and branch commits are
  unchanged by releases.
  A fresh tag history starts at `1.0.0-alpha.1`, not the Cargo placeholder version.
  Publishing follows successful checks on release-branch pushes. The SHA-pinned
  semantic-release action supplies the release tooling; there is no npm project.


The current setup files are `rust-toolchain.toml`, `.editorconfig`,
`.gitattributes`, `.gitignore`, `renovate.json`, `deny.toml`,
`.github/workflows/ci.yml`, `.githooks/pre-commit`, `.githooks/commit-msg`,
`committed.toml` and `release.config.mjs`. Cargo.toml has no custom lint table;
CI invokes Clippy with warnings denied.

## Verification and remaining setup

- Local formatting, Clippy and all eight core tests have passed.
- Hook acceptance/rejection was checked locally. No actual project tag or release
  was created.
- Alpha-to-stable tagging was verified locally during setup. No custom release
  test scripts are retained. GitHub publishing requires a hosted canary.
- Workflow YAML was parsed locally; consult GitHub Actions for remote results.
- cargo-deny advisories, licenses, bans and sources passed locally on 2026-11-07.
- Hosting and merge methods are configured. Renovate activation and branch
  protection remain to be configured; release permissions need hosted validation.
- No service/UAC or end-to-end Windows canary has run.

## Deferred until needed

Keep one package until a second package or consumer justifies a workspace. Use
standard Cargo commands; nextest, just and custom rustfmt settings are unnecessary
for the current suite. Signing, installers, executable release artifacts and a
distribution license remain undecided.

Add files when their behavior is implemented, rather than empty scaffolding:

- `src/protocol.rs` and `src/reconcile.rs` when the library grows.
- `src/bin/rustdesk-agent.rs` and `src/windows/` for the Windows service, verified
  RustDesk CLI adapter and credential handling.
- `src/management.rs` and `src/state.rs` for polling/reporting and durable identity
  and revision state.
- `schemas/` for desired-state/status JSON schemas and consistency tests.
- `scripts/windows/`, `docs/windows-validation.md` and `docs/operations.md` for
  installation, runtime evidence and rollout/rollback instructions.

No backend package until console v0.2.40 is inspected and the integration approach
is chosen. The repository remains an incomplete library, with no deployable agent,
production changes or credential provisioning.
