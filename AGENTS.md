# RustDesk agent development

- V1 targets Windows x64 only. Keep the core testable on the development host.
- Read `docs/plan.md` and `docs/contract.md` before implementation. Clearly distinguish implemented behavior from proposals and unverified assumptions.
- Permit only the documented RustDesk settings. No arbitrary remote commands, direct TOML editing, custom RustDesk fork, or permanent-password handling without verified secure support.
- Validate the exact RustDesk CLI and Windows service context before enabling writes. Exit status alone is insufficient verification.
- Never commit device credentials or deployment secrets. Do not modify production hbbs/hbbr/Caddy, change credentials, or deploy without explicit instruction.
- Use the installed `codebase-memory` skill for structural codebase exploration.
- Run `cargo fmt --check` and `cargo test --locked` before committing code changes. Windows-specific changes also require the Windows x64 compile check and appropriate runtime tests; document tests that cannot run locally.
