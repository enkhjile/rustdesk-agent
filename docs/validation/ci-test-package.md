# Downloaded CI package validation

## Scope and inputs

The downloaded Windows x64 package from [CI run 42754455756](https://github.com/enkhjile/rustdesk-agent/actions/runs/42754455756)
passed the isolated service checks below. Artifact ID: `13735365521`; source
checkout: `e748c90b3d161a99f0c3c0b9550fa4fd19495caa` (PR merge commit).
All four entries in `SHA256SUMS` matched the downloaded files.

The package uses Rust 1.99.1, release profile, target
`x86_64-pc-windows-msvc` and no optional features. Its executable SHA-256 is
`0266e55da0a6724f8a3573f16f9087d45d3fee67453d5bc20023861971a046d5`.
The upgrade baseline was built locally from tag `v1.0.0-alpha.12`, with the same
toolchain, target and feature options; its SHA-256 is
`6a01dbf1fecb2a5516cfa0fd6765bdf13717afa5c7b07e8b03c4a8570da3b5c4`.
It was not a downloaded release asset.

The VM was `RustDeskAgent-Validation`, Windows 11 Pro 10.0.26200, with zero
network adapters and official RustDesk 1.4.7. Setup ran as `RDAGENT-LAB\LabAdmin`;
the service ran as LocalService with its service SID. Dummy credentials and
trusted loopback HTTPS connected the service to a local Console build whose
source matches merged Console `7c32522`. Console administration used its API.

## Results

| Behavior | Observed result |
| --- | --- |
| Stopped upgrade from alpha.12 | Packaged installer replaced the executable with the downloaded bytes. Enrollment/state hashes and ACLs were unchanged; the service stayed stopped. |
| Pending report across upgrade | Baseline revision 1, sequence 1 remained durable after a dropped acknowledgement. The new executable retried the identical body after Console restart; Console accepted it with 204. |
| Policy removal and reassignment | Removal produced heartbeat sequence 2 while retaining revision 1. Reassignment produced revision 2/sequence 3; a second removal produced heartbeat sequence 4; reassignment produced revision 3/sequence 5. |
| Credential rotation | Console rejected the old credential with 401. Packaged `--replace-credential` preserved state bytes and enrollment ACLs; the same UUID resumed reporting at sequence 6. |
| Uninstall and reinstall | Protected data and ACLs survived unchanged. Installing the downloaded executable again resumed reporting at sequence 7 without reenrollment or initialization. |
| Running upgrade | The packaged installer restarted the previously running service. Reporting advanced to sequence 8 with the same UUID and revision 3. |
| Revocation | Poll returned 401; Console's sequence and last contact did not advance. |
| Fresh package installation | A separate dummy registration completed install, enrollment, UUID binding and administrator initialization. With no policy, the packaged service sent heartbeat sequence 1 without an accepted revision. |
| Settings and cleanup | Automatic writes remained disabled; assigned reports were `unsupported` with null applied revision. Before/after detection output matched. The checkpoint was restored and removed, and the VM stopped with zero adapters. |

Installer operations used the script from the downloaded package:

```powershell
$hash = (Get-FileHash C:\Lab\CiPackage\agent-new.exe).Hash
& C:\Lab\CiPackage\scripts\install.ps1 -Action Upgrade -SourcePath C:\Lab\CiPackage\agent-new.exe -Sha256 $hash
& C:\Lab\CiPackage\scripts\install.ps1 -Action Uninstall
& C:\Lab\CiPackage\scripts\install.ps1 -Action Install -SourcePath C:\Lab\CiPackage\agent-new.exe -Sha256 $hash
```

Enrollment used `--enroll https://localhost:8444`, then Console UUID binding and
`--init-state`. Rotation used `--replace-credential`. Credentials were supplied
only through redirected stdin, never arguments or captured logs.

## Automated checks and limits

Hosted CI passed formatting, Clippy, default/lab-feature tests, dependency checks,
the release build and installer lifecycle against the packaged executable.
All compiled default-feature test executables also passed as administrator in
the VM, including protected-storage tests unavailable to the non-elevated host.

This validates a CI test artifact, not a signed distribution or an alpha.13
release executable. No production networking, browser UI, live settings verifier,
power-loss recovery or downgrade was tested here. Signing, distribution licensing
and dependency notices remain release gates. CI artifacts have limited retention;
the source commit and hashes identify the tested bytes after expiry.
