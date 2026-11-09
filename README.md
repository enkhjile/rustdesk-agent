# RustDesk Windows agent — development slices

Windows x64 companion agent in development. The service polls desired state over
HTTPS and delivers durable status reports, but does not apply settings automatically.
An explicit local `--set-option` command supports two allowlisted settings using
official RustDesk 1.4.7. Matching CLI readback is separate from live verification.
No custom client, direct TOML editing or password provisioning.

[Enrollment](docs/enrollment.md) uses machine-scoped DPAPI and service-SID ACLs.
Enrolled services require administrator [state initialization](docs/state.md).
Isolated VM tests passed service access isolation, ordinary-user denial, HTTP
retries/cancellation and unattended reboot. Explicit setters passed under
administrator and LocalService accounts, including restart/reboot persistence.
See the [dated validation records](docs/validation/README.md) for exact scope.

Production backend/server registration, automatic settings application and
deployment packaging remain unfinished. Start with [Windows usage](docs/windows-service.md),
the [current plan](docs/plan.md), and the [contract fixture](docs/contract-testing.md).

## Repository layout

```text
Cargo.toml          Single Rust package: library and Windows executable
Cargo.lock          Locked dependencies; keep this tracked
src/lib.rs          Desired-state parsing, reconciliation and backoff
src/detection.rs    Read-only query allowlist and output parsing
src/probe.rs        Bounded CLI process runner
src/setter.rs       Explicit allowlisted local settings command
src/windows/       Windows SCM lifecycle and protected enrollment storage
src/management.rs   Authenticated polling and durable status delivery
src/state.rs        Validated revision/sequence/outbox snapshot
src/enrollment.rs   Validated device identity, origin and credential record
tests/reconcile.rs  Tests using an in-memory settings adapter
docs/plan.md        Current capabilities and remaining validation gates
docs/validation/   Dated test evidence, not current operating instructions
docs/contract.md    Management endpoint contract (test backend only)
tests/contract_backend.rs  HTTPS contract tests
tests/support/     In-memory Rust test backend
.github/workflows/ci.yml  Windows checks, dependency audit, automatic releases
.githooks/         Local formatting and commit-message checks
committed.toml     Commit-message policy
release.config.mjs Semantic-release branches and plugins
renovate.json      Dependency and action update configuration
deny.toml          Dependency policy
.editorconfig      Editor whitespace defaults
```

## Development

Install Rust through rustup. The repository selects Rust 1.99.1 with rustfmt and
Clippy through `rust-toolchain.toml`. Windows builds require the MSVC C++ build
tools and Windows SDK. From this directory:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
```

On a host with the Windows MSVC C++ build tools and SDK, check Windows x64 with:

```sh
rustup target add x86_64-pc-windows-msvc
cargo check --locked --target x86_64-pc-windows-msvc
```

Add `--offline` to Cargo commands when dependencies are cached. The TLS dependency also needs Windows C headers; installing the Rust target alone
is insufficient for cross-checking from macOS. A cross-target
check does not run Windows tests or establish service/UAC behavior. The core can
be tested on the development host; the agent executable targets Windows x64 only.
Two protected-storage tests require an elevated Windows test runner.

The public repository is [enkhjile/rustdesk-agent](https://github.com/enkhjile/rustdesk-agent).
Use short-lived branches targeting `alpha`; promote validated changes to `main`.
No kiosk deployment automation is configured.

Dependency updates use Renovate (`renovate.json`): routine Cargo and GitHub Actions
updates are grouped separately, with automatic merging disabled. Enable Renovate
on the remote repository after hosting is configured; local configuration alone
does not activate the bot.

## CI

One GitHub Actions job runs formatting, Clippy, tests and a build on Windows x64
for pull requests and pushes to `main` and `alpha`. It uses the repository's pinned toolchain
and lockfile, using `actions-rust-lang/setup-rust-toolchain`. It can also be
started manually. The Rust test suite includes the loopback HTTPS contract tests;
no separate runtime or certificate tool is required. Actions are pinned to full commit SHAs. Release automation runs after checks, as described below. No kiosk deployment is configured. Hosted checks do not validate RustDesk service behavior or UAC.

A separate Linux CI job runs cargo-deny (its action uses Docker) to check the
locked dependency graph for advisories, licenses and unapproved sources. This
checks dependencies for all targets, including Windows; it does not add Linux
agent support. No advisories are ignored. The license policy allows MIT,
Apache-2.0, Unicode-3.0, ISC, BSD-3-Clause, Zlib and CDLA-Permissive-2.0 dependencies
(the latter four cover the HTTP/TLS stack and root certificate data); the unpublished application itself is
excluded from license checks until a distribution license is selected.

Run the same checks locally:

```sh
cargo install cargo-deny --version 0.20.2 --locked
cargo deny --locked --all-features check
```

The audit needs network access to refresh advisory data. cargo-deny passed
locally on 2026-11-07. See the repository Actions tab for remote CI results.

## Git hooks and releases

Install the commit checker, then enable this repository's hooks:

```sh
cargo install committed --version 1.1.12 --locked
git config --local core.hooksPath .githooks
```

The pre-commit hook checks formatting without rewriting files. The commit-msg
hook requires Conventional Commits, such as `feat: add polling` or
`fix: reject stale revisions`. Git for Windows provides the shell for these
hooks. Commit-message validation runs locally only; CI checks the Rust code and
dependencies. The checker accepts `feat`, `fix`, `chore`, `docs`, `style`,
`refactor`, `perf` and `test`; use `chore` for CI changes. Subjects are limited to
72 characters. Hooks must be enabled in each clone and can be bypassed, so review
the final PR squash message as well.

Use squash merges for feature/fix PRs targeting `alpha`, with a conventional
squash message. Promote `alpha` into `main` using a merge commit (not squash or
rebase) to preserve release history. Merge stable fixes back into `alpha` the same
way. Merge commits still need a conventional subject, for example
`chore: promote alpha to stable`; replace Git's default merge message. To disable local hooks:
`git config --local --unset core.hooksPath`.

## Releases

semantic-release runs after checks on pushes to `alpha`
and `main`. Merging a
release-worthy change publishes directly; there is no release PR. `alpha` creates
GitHub prereleases, and `main` creates stable releases. No npm or crates.io package
is published, and there is no executable artifact or deployment yet.

With no prior release tags, semantic-release starts at `1.0.0-alpha.1` on `alpha`
and `1.0.0` on `main`. The checked-in `0.0.0-development` Cargo version is a development
placeholder, not a version seed for semantic-release. Version selection follows
Git history using semantic-release's default parser: `fix:` and `perf:` bump patch,
`feat:` bumps minor, and a `BREAKING CHANGE:` footer bumps major. Use that footer
for breaking changes; `!` in the subject alone is not recognized by the default
parser. Alpha releases receive a prerelease counter. No artificial baseline tags
are created by setup.

Releases create Git tags and GitHub release notes without modifying Cargo.toml,
Cargo.lock or branch commits. The tag points to the commit that passed CI. The
Cargo version remains a development placeholder; tags and GitHub Releases identify
released versions. No generated CHANGELOG.md is committed. When executable
packaging is implemented, its displayed version can be supplied during the CI
build without committing version changes back to either branch.

Releases run through the SHA-pinned `cycjimmy/semantic-release-action`. The action
provides its own Node runtime and installs the latest semantic-release by default.
No local Node installation, package.json, npm lockfile or custom release scripts
are needed. `release.config.mjs` explicitly selects GitHub-only plugins; the npm
publishing plugin is excluded. Renovate maintains the action pin. Release-tool
transitive dependencies are managed by the action rather than a repository lockfile;
there is no repository-level npm audit job.

When connecting GitHub:

1. Publish both `main` and `alpha`, and use `alpha` as the development PR target.
   Allow squash and merge commits; protect both branches with CI checks.
2. The release identity needs contents-write access for tags and GitHub releases.
   It does not need to push version commits or bypass branch protection for them.
   The default `GITHUB_TOKEN` is used unless a scoped `RELEASE_TOKEN` is supplied.
   Tag rules must allow the release identity to create release tags. No PR/issue
   comments or issue creation are enabled.
3. Releases run automatically after successful checks on pushes to `alpha` and
   `main`. PR and manual workflow runs never publish. Keep unfinished work on
   `alpha` until stable promotion is intended. To pause releases, disable the
   release job in the workflow.

Existing released versions must never be retagged; release a new fix instead.
Automatic alpha prerelease publication has succeeded. That does not establish
installer readiness or verify every protected-branch policy.


References: [committed](https://github.com/crate-ci/committed),
[semantic-release branches](https://semantic-release.org/foundation/workflow-configuration/),
[why release commits are discouraged](https://semantic-release.org/support/faq/#making-commits-during-the-release-process-adds-significant-complexity).
