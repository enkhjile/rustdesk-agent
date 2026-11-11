# Windows x64 unsigned package

This first distribution is intentionally unsigned. Start with a noncritical pilot
device and keep another way to access it. Windows may display publisher or
SmartScreen warnings; do not disable system security controls globally.
The agent is MIT licensed; LICENSE, third-party notices and Rust standard-library
notices are included. RustDesk and Console are separate installations and are not
relicensed or bundled here. The normal build includes managed application.
Install defaults to Managed; add `-Mode ObserveOnly` for reporting only. Upgrade
preserves the existing mode unless `-Mode` is supplied.

Download `rustdesk-agent-windows-x64-unsigned.zip` and `SHA256SUMS` from the intended
GitHub release. Alpha tags are prereleases, not stable releases. The identical ZIP
is retained for seven days in the `rustdesk-agent-windows-x64-<commit>` CI artifact.
Check the release/run's source and all jobs before
executing it; a pull-request artifact contains unmerged code. `build.json` records
the actual checkout commit (the merge commit for PR runs), run URL, target,
compiler, build options and unsigned status. The executable's development version
is not the release identifier; use the release tag and source commit in build.json.

Before extraction, compare the ZIP's SHA-256 with the downloaded `SHA256SUMS`.
Extract it into an administrator-controlled staging directory. The ZIP contains
`rustdesk-agent.exe`, `install.ps1`, this README, license notices, `build.json`,
and an inner `SHA256SUMS` covering those files. Verify those hashes too. A matching
checksum detects changed bytes; it does not replace code signing. In elevated
64-bit Windows PowerShell 5.1, after verification:

```powershell
$hash = (Get-FileHash .\rustdesk-agent.exe -Algorithm SHA256).Hash
.\install.ps1 -Action Install -SourcePath (Resolve-Path .\rustdesk-agent.exe).Path -Sha256 $hash
# For an existing installation, use -Action Upgrade instead.
```

Install leaves the service stopped. Enroll with a Console-issued device credential
on redirected stdin, bind the printed UUID in Console, and run `--init-state`
once as administrator before starting the service. Never put credentials in
command arguments. Upgrade preserves enrollment/state and restores the previous
running/stopped service status; do not enroll or initialize again.

See the repository's `docs/installer.md` and `docs/enrollment.md` at the source
commit in `build.json` for the full setup and recovery procedures. Official
RustDesk 1.4.7 under Program Files is the validated client. This package does not
contain RustDesk or Console. Reported CLI values are not verified live settings.

For password changes, Console also needs its dedicated password-encryption key
and the supporting password-management version. See `docs/password-management.md`.
Test check-in, an approval/password-type policy, an explicit password change,
service restart and machine reboot on the pilot device. Keep a working access
method and confirm normal RustDesk connections before expanding the rollout.
