# Agent IP reporting and Console location

Implemented locally in the agent checkout and the separate Console checkout at
`target/console-upstream`. No production deployment or service installation.

## Behavior

- Windows `GetAdaptersAddresses` collects preferred unicast IPv4/IPv6 addresses
  from operational adapters. Results exclude loopback, unspecified, multicast
  and unscoped IPv6 link-local addresses; deduplicated, sorted and capped at 64.
- Optional `local_ips` is captured when a fresh heartbeat or policy report is
  created and persisted with the report. A retry retains the original snapshot.
  Enumeration failure omits the field without stopping management.
- Console validates the list and keeps request source IP/receipt time outside
  the report's idempotency identity. Only accepted authenticated reports update
  that metadata. Forwarding headers require explicitly trusted proxy IPs.
- Device details show the addresses, receipt time and an explicitly approximate
  location from the local City MMDB. Missing metadata/location is unavailable.
  English and Chinese labels are included. No Windows Location Services calls.

## Checks performed on Windows x64

- `cargo fmt --check`: passed for agent.
- `cargo check --locked --target x86_64-pc-windows-msvc`: passed for agent.
- Native adapter enumeration test: passed on the development host.
- New durable snapshot test first failed, then passed after implementation;
  verifies new fields survive restart and old reports remain unchanged.
- `cargo test --locked`: passed in an elevated Windows test process, including
  all 50 library tests and the binary, integration and documentation targets.
  The earlier three protected-storage failures were caused by a non-elevated
  runner; no security checks or tests were weakened or skipped.
- Agent `cargo clippy --locked --all-targets --all-features -- -D warnings`: passed.
- Console `cargo test --locked --workspace`: passed, including 253 server tests.
  Fixed its Windows MMDB replacement to use write-through `MoveFileExW` instead
  of deleting the destination before rename and attempting Unix directory sync.
  A regression test first reproduced loss of the old file on failure, then
  passed with the fix. The local-record-path assertion now compares native paths.
- Console tests cover optional/invalid/oversized/duplicate IP lists, forwarding
  trust, source-IP changes on identical retries, rejected-report preservation,
  missing GeoIP fields and a repeatable migration preserving existing reports.
- Web Vitest suite: 36 passed. TypeScript build and Vite production build passed
  (existing large-chunk warning). Output was written under Console's `target`.
- Browser check used the actual AgentPanel with illustrative API fixture data
  in an isolated Chrome context. At 390px and 1280px widths, IPs and location
  rendered without horizontal overflow; no browser errors or warnings.
- `git diff --check`: passed in both checkouts.
- Review mutation check: inverting the address loopback filter made its focused
  test fail. The original implementation was restored.

## Remaining runtime gates

Test enumeration/report delivery in the installed LocalService context and
after reboot on the target fleet. SQLite migration was tested; MySQL/PostgreSQL
runtime migration was not. Real GeoIP database provisioning and a production
reverse-proxy chain were not exercised. Geolocation record mapping was tested
with fixtures; a real City MMDB lookup remains a deployment validation step.

Upgrade Console before the agent. Provision the City database and configure
`CONSOLE_AGENT_TRUSTED_PROXIES` as documented in Console's
`docs/companion-agent.md`. Production deployment remains separately authorized.
