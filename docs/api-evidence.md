# Console API evidence

Inspected 2026-11-08, without contacting production or using a Console credential.

Source: [v0.2.40 release OpenAPI](https://github.com/dockers-x/rustdesk-console/releases/download/v0.2.40/rustdesk-console-openapi.json).
Tag resolves to `52c64bf207f7da61b3977d186eb47b5b324bcd96`.
Downloaded asset SHA-256: `61b8a6b04070492fe852e9ad10ec59de8baa210521a2fbe59807650aa87dd3ee`.
The document declares API version `1.0.0` under `/api/v1`; that is distinct from
Console release version `v0.2.40`.

The complete published path inventory covers `/me`, `/users`, `/users/{id}`,
`/devices`, `/devices/{id}`, `/device-groups`, `/address-books`,
`/address-books/{id}` and `/strategies`.

- Device PATCH accepts only `alias`, `group_id`, `user_id`; unknown fields are forbidden.
- Strategy GET returns IDs and names. Its description explicitly excludes editing.
- Device path IDs are numeric Console row IDs, not RustDesk connection IDs.
- Authentication uses scoped `rdc_` API keys tied to Console accounts; administrative
  writes recheck the owner's role. These are not the proposed per-device credentials.
- No published desired-state revision storage, device-bound enrollment or agent
  outcome reporting is exposed by this developer API. This finding is about the
  published API, not a claim that every internal Console route was audited.

## Minimal proposed addition

The test backend implements the two routes in [contract.md](contract.md).
A production backend or future Console extension would store enrolled UUID and credential hash,
immutable desired revisions and the latest idempotent status report. An admin
assigns a new revision; agents can only read their own assignment and post their
own status. Keep any Console integration credentials on the backend, never on
kiosks. Map UUID to the reported RustDesk ID without treating that ID as proof
of identity. A loopback-only test fixture now implements the agent contract in `tests/support/`. Production backend implementation and deployment are still pending.

## Existing Windows setup integration

The kiosk repository's `external/pk-libs-rs/crates/installer-tools-cli/src/rustdesk_setup.rs`
embeds RustDesk 1.4.7, invokes MSI installation, and runs `--option key value`.
It checks exit status without independently reading settings back. It also passes
`--password` as a process argument; do not reuse that path for secret delivery.
The installer remains unchanged. Subsequent [isolated setter tests](validation/2026-11-09-official-cli-setters.md)
passed with the official client under both administrator and LocalService accounts.
Independent live verification and connection behavior remain separate gates.
