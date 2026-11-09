# Default-off managed application

## Environment and commands

Disposable `RustDeskAgent-Validation` (`RDAGENT-LAB`), Windows 11 Pro 25H2 build
26200.6584, no network adapters. Setup ran as LabAdmin; the actual `RustDeskAgent`
service ran as `NT AUTHORITY\LocalService` with its service SID. Existing dummy
enrollment and the trusted `https://localhost:8443` lab endpoint were reused.
No enrollment/state reset or real credential change was performed.

Official installed RustDesk 1.4.7 had SHA256
`7A43F30721826E6D6B11686A47B3F6D672DC58C1BDB2B62076D704E8BA5235C0`.
Rust 1.99.1 produced these Windows x64 debug binaries:

| Build | SHA256 |
| --- | --- |
| Default | `2AD5BD556785C5CA91AC13D44A6E6816262F0BA409CF3B3807037219C91D247C` |
| `lab-apply` | `CB1692A8615FF324A9B01B97834AD04BF8C1B8E9F7B8E6BF921E2B61885D5DE0` |

```powershell
cargo build --locked --target x86_64-pc-windows-msvc
cargo build --locked --target x86_64-pc-windows-msvc --features lab-apply
```

The experimental executable was installed temporarily in the existing service
configuration with this command line:

```text
C:\Lab\rustdesk-agent-lab.exe --service "C:\Program Files\RustDesk\rustdesk.exe" C:\ProgramData\RustDeskAgent\managed.json --lab-apply
```

The backend was a disposable loopback HTTPS harness using dummy bearer
authentication and revision/sequence checks, not a deployed management server.
Cross-account checks captured the official `--option approve-mode` and
`--option verification-method` commands' output and waited for process exit.

## Results

- Default build rejected the startup flag. The experimental build without the
  flag reported `unsupported` and left both original settings unchanged.
- With both gates enabled, desired `password-click` and
  `use-temporary-password` produced matching CLI output under the service and
  administrator accounts. Reports said `unverified_cli_match`, never `verified`;
  applied revision stayed null.
- Changing approval externally to `click`, then polling the unchanged desired
  revision, restored `password-click` in CLI readback.
- A higher revision managing only approval changed it to `click` and preserved
  verification mode. Stale revisions, changed same-revision content and HTTP 401
  caused no application or sequence advance.
- A failed status POST left a durable pending report. After restarting the
  service, the retry body was byte-identical. GET 204 did not apply cached state
  or advance the observation sequence.
- Holding the agent mutex allowed the already-matching approval result to remain
  `unverified_cli_match` while verification reported `write_failed`. After releasing
  the mutex, a fresh poll successfully retried verification.
- Stopping the service while its setter worker waited on that mutex completed
  within the five-second limit and removed the worker. The saved verification
  result remained `indeterminate`, with null applied revision. Restart delivered
  that report without reapplying after GET 204.

Both accounts' original empty option values were restored while RustDesk was
running and stopped, then rechecked after restart. The original service executable
arguments and automatic startup mode were restored. Temporary helper services and
the experimental endpoint task were removed, the pending report was drained,
and the network-isolated VM was shut down. Durable revision/sequence history was
retained. No permanent password, TOML editing or custom RustDesk client was used.

## Automated checks and limits

Formatting, all-feature Clippy and both Windows builds passed. Full elevated
guest unit suites passed: 28 library tests and 8 binary tests. Host tests and
integrations passed excluding the two administrator-only protected-store tests;
an unfiltered host `cargo test --locked` failed only those two permission checks.
CI checks both default and experimental configurations.

Unit tests additionally cover interrupted/persist-failed application, partial
report restart, invalid desired payloads and rejection of non-loopback origins.
This run did not independently verify live connection behavior, perform a new
reboot test, or force-kill the managed service during an actual RustDesk write.
The stop test interrupted a waiting worker; earlier records cover forced death
of the explicit setter. CLI-cache ambiguity, external writers, nontransactional
settings and physical power-loss recovery remain limits. Production application
is disabled; these results do not authorize rollout.

Raw logs and the runner are retained under ignored
`target/validation/managed-final/`.
