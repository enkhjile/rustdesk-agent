# Guided setup and Console enrollment

Tested in the existing network-isolated `RustDeskAgent-Validation` VM, Windows 11
Pro build 26200, under `RDAGENT-LAB\LabAdmin`. The installed agent runs as
`NT AUTHORITY\LocalService`. Console runs only on guest loopback, behind the lab
HTTPS endpoint `https://localhost:8444`, with its lab certificate trusted in the
guest. Only disposable credentials were used. No policy was assigned and setup
selected observe-only mode. The VM checkpoint was restored, then the VM was off
with zero network adapters.

## Passed

- Fresh install using the setup executable's embedded engine: service creation,
  encrypted enrollment, automatic UUID binding, state initialization and startup.
- Console check-in changed from pending (202) to received (204) after its first
  real service heartbeat. Console UUID matched the persisted local UUID.
- Repeating `rustdesk-agent.exe --finish-setup` preserved enrollment/state bytes
  and ACLs; Console accepted the identical binding.
- Stopped-service upgrade preserved both protected files and ACLs exactly,
  retained observe-only mode and left the service stopped.
- Running-service upgrade restarted the service, preserved identity and mode,
  and delivered a greater observation sequence to Console.
- Corrupt existing state was rejected and not overwritten.
- Missing established state was refused rather than reinitialized.
- A setup interrupted before its first service start resumed successfully.
- Uninstall retaining enrollment/state followed by guided reinstall started the
  new service and kept the same identity.

The initial upgrade exposed a missing `System.ServiceProcess` assembly load in
a clean Windows PowerShell process. The installer now loads it explicitly; the
corrected VM run passed. A clean-process upgrade regression is included in
`tests/installer.ps1` for hosted Windows CI.

## Reproduction and limits

Build with `cargo build --locked --release --target x86_64-pc-windows-msvc`, then
`scripts/build-setup.ps1 -BinaryPath <agent.exe> -NoticesPath <notices.txt> -OutputPath <setup.exe>`.
The guest loaded that exact setup assembly and invoked its internal `Install`
method with `(HTTPS origin, dummy device credential, true)` for first install,
then `('', '', false)` for upgrades. This exercises the engine used by the button,
including its embedded agent and script, without placing tokens in arguments.
Local preserved evidence lives under `target/validation/guided-setup`; it is not
part of the distribution.

Automated tests cover endpoint authentication, immutable/idempotent binding,
UUID uniqueness, disabled/revoked devices, check-in status, TLS request identity,
HTTP errors and redirect rejection. Package checks verify embedded executable
hash, notices and the administrator manifest. Decision tests also cover
fresh/upgrade/reinstall decisions and refusal when both stores
are missing on an existing unmarked service. Interactive visual/UAC presentation
and a real-machine pilot remain separate gates; the VM engine test does not claim
to exercise physical clicks or SmartScreen. The installer remains unsigned.
