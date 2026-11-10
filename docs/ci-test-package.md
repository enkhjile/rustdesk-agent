# Windows x64 CI test package

For disposable lab validation only. This unsigned package is not a production
installer or a GitHub release asset. Distribution licensing, dependency notices
and signing remain unfinished. Automatic settings writes are disabled: the build
uses `--no-default-features` without `lab-apply`.

Download the `rustdesk-agent-test-windows-x64-<commit>` artifact from a successful
CI run in `enkhjile/rustdesk-agent`. Check the run's source and all jobs before
executing it; a pull-request artifact contains unmerged code. `build.json` records
the actual checkout commit (the merge commit for PR runs), run URL, target,
compiler and build options. Artifacts expire after seven days.

The archive contains `rustdesk-agent.exe`, `install.ps1`, this README,
`build.json` and `SHA256SUMS`. Extract it into an administrator-controlled
staging directory. Before running the installer, verify every listed SHA-256
against the downloaded files. A matching checksum detects changed bytes; it does
not authenticate the publisher. In elevated 64-bit Windows PowerShell 5.1:

```powershell
$hash = (Get-FileHash .\rustdesk-agent.exe -Algorithm SHA256).Hash
.\install.ps1 -Action Install -SourcePath (Resolve-Path .\rustdesk-agent.exe).Path -Sha256 $hash
# For an existing installation, use -Action Upgrade instead.
```

Install leaves the service stopped. Enroll with a dummy Console-issued credential
on redirected stdin, bind the printed UUID in Console, and run `--init-state`
once as administrator before starting the service. Never put credentials in
command arguments. Upgrade preserves enrollment/state and restores the previous
running/stopped service status; do not enroll or initialize again.

See the repository's `docs/installer.md` and `docs/enrollment.md` at the source
commit in `build.json` for the full setup and recovery procedures. Official
RustDesk 1.4.7 under Program Files is the validated client. This package does not
contain RustDesk or Console. Reported CLI values are not verified live settings.
