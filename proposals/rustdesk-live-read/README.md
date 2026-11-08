# Upstream proposal: explicit live settings read

This is a reviewable RustDesk change proposal, **not an agent feature or a shipped
fork**. Nothing in the agent's executable includes this directory. No writes are
enabled. PRs #8 and #9 are independent and remain unmerged.

## Problem and command

RustDesk 1.4.7's `--option key` can silently return cached options when IPC fails.
A successful exit therefore cannot prove a live read. Direct external IPC is not
a workaround: Windows authorizes the RustDesk executable as the peer.

Propose Windows-only `rustdesk.exe --get-settings-json`, implemented inside
RustDesk and subject to its existing installation, privilege and CLI-settings
policy checks. The new path sends one `Options(None)` request and accepts only an
`Options(Some(map))` reply. It neither reads nor updates the caller's option cache.
Existing `--option` behavior and IPC authorization are unchanged.

Success exits 0 and prints one JSON object:

```json
{"schema_version":1,"source":"live_ipc","settings":{"approve_mode":"password-click","verification_method":"use-both-passwords"}}
```

Only those two keys can be printed; other options, credentials, server addresses
and passwords are excluded. Values are raw server option values, **not resolved
effective settings**: a missing key is `null`, an explicit empty string stays
`""`. Consumers must not infer defaults, password existence, or connection
behavior from these states. Unknown nonempty values fail closed rather than being
normalized. `live_ipc` means a reply from the running RustDesk IPC server, not a
claim that a remote session or the security policy has been exercised.

| Exit | stderr | Meaning |
| --- | --- | --- |
| 2 | `usage: --get-settings-json` | Extra or incorrect arguments |
| 3 | `settings read denied` | Installation, privilege or policy check failed |
| 4 | `live settings unavailable` | Connection/send/receive failure, wrong reply, EOF, or whole-exchange one-second timeout |
| 5 | `unsupported live setting value` | Unexpected nonempty allowlisted-key value |

Failures return no success JSON and do not print raw transport errors or rejected
values. The command has no setter form. Startup code can still perform RustDesk's
normal internal initialization; this proposal does not promise a globally
filesystem-write-free RustDesk process.

## Patch and executable tests

`rustdesk-1.4.7.patch` targets official tag 1.4.7, commit
`0c86d4616298f09435f6236599b300964aa61460`. It adds the CLI dispatch, a strict IPC
adapter, and `src/settings_read.rs`. The adjacent `settings_read.rs` is the exact
proposed module, compiled by `tests/upstream_live_read.rs` in this repository.
The test harness aliases its Tokio dependency to RustDesk's `hbb_common::tokio`
import; it does not pretend to implement RustDesk or its IPC authorization.

In a source checkout at that commit:

```powershell
git apply --check /path/to/rustdesk-1.4.7.patch
git apply /path/to/rustdesk-1.4.7.patch
```

In this agent repository:

```powershell
cargo test --locked --test upstream_live_read
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```

The tests compile the proposed module itself and cover all allowed value
combinations, missing versus empty keys, output allowlisting, denied requests
without invoking IPC, unknown-value rejection, fixed error codes, transport
failure, and the one-second timeout. Four tests failed against the initial stub
and passed after implementation. No production dependencies were added.

The patch was application-checked against the pinned original files and its
modified Rust files parsed/formatted with rustfmt. **The full upstream RustDesk
application has not been built or run with this patch.** The adapter, CLI stream
handling, actual EOF/wrong-message behavior, server authentication, LocalService
policy and installed Windows service context still require upstream integration
tests and an isolated VM run. The agent's hosted tests cannot establish those.
An upstream maintainer must also review interaction with custom-client policies,
default resolution and current upstream HEAD before accepting this interface.

This proposal has not been submitted to the RustDesk project. Submission and any
experimental upstream binary build/install are separate steps; no binary or
release is published here. The agent should adopt the command only after an
official supported version and its exact service context pass validation.
