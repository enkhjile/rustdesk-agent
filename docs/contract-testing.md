# Contract test backend

Run the Rust HTTPS contract tests from the repository root:

```sh
cargo test --locked --test contract_backend
```

They also run with the normal `cargo test --locked` command, which Windows CI
is configured to execute. Local verification was on macOS; Windows execution
has not been verified here.
The fixture is in `tests/support/mod.rs`; all HTTP, TLS, certificate-generation
and JSON Schema dependencies are dev-dependencies. They are not linked into a
normal agent build. No second language, external certificate tool or extra CI
job is needed.

`Fixture::start()` binds HTTPS to a random port on `127.0.0.1`. It generates a
localhost certificate and private key in memory. Its client trusts that
certificate explicitly, validates TLS, disables proxies/redirects and uses a
three-second request timeout. Dropping the fixture stops the server; no key
files or system trust changes are made.

Two hardcoded disposable test tokens map to two test UUIDs. `assign(uuid, state)`
validates desired state with the agent parser and checked-in schema, rejects
stale/conflicting revisions and stores it in memory. There is no remote admin
route. GET `/v1/agent/desired` returns 200 or 204 for an unassigned device.
POST `/v1/agent/status` validates the status schema and semantic constraints,
rejects duplicate keys and returns 204 for accepted/latest identical reports.
Stale/conflicting reports return 409. Invalid tokens return 401, mismatched device
identity 403, malformed status 400 and bodies larger than 64 KiB 413.

The six tests cover device isolation, immutable revisions, schema rejection,
report ordering/retries and real HTTPS trust checks. This fixture is not a
production server or a Windows/RustDesk end-to-end test. State and reports are
in memory; credential hashing, rotation/revocation, durable storage, production
server hardening, audit logging and heartbeat reporting remain out of scope.

References: [axum-server TLS](https://docs.rs/axum-server/0.9.0/axum_server/tls_rustls/index.html),
[rcgen](https://docs.rs/rcgen/0.14.10/rcgen/),
[JSON Schema validation](https://docs.rs/jsonschema/0.60.2/jsonschema/).
