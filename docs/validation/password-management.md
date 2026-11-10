# Console-to-agent password operations

## Environment

Two disposable Hyper-V guests, `RustDeskAgent-Validation` and `RustDeskAgent-Peer`,
used a private switch with no host/Internet uplink or default route. Console ran
on the Peer; the agent used `https://192.0.2.11:8444` with a guest-only trusted
certificate and dummy device credentials. Setup/tests ran as `LabAdmin`;
`RustDeskAgent` ran as `NT AUTHORITY\LocalService` with its service SID enabled.
The guests reported Windows build 26200 (`WindowsProductName: Windows 10 Pro`,
`WindowsVersion: 2009`). Official RustDesk was 1.4.7; its hash and separate
connection/password-replacement evidence are in [command validation](password-command.md).

Rust 1.99.1 development builds tested:

| Binary | SHA-256 |
| --- | --- |
| Agent, Windows x64 | `082B45E07EB23257CBCA3307EE4D59C4C8E165D7597480CAB79413E6E3571783` |
| Console | `27E6CD2BBDB3869AA420B93E2C3A52C7DAB5F6CDBDC8E718251E35F9D576EDA7` |

## Completed behavior checks

| Check | Result |
| --- | --- |
| Observe-only | A queued password remained queued through an authenticated heartbeat; no password attempt was persisted. |
| Managed execution | After an explicit mode upgrade, the real service claimed the operation and reported `command_accepted` from the official CLI. |
| Lost acknowledgement | The HTTPS proxy forwarded the first result but dropped its response. The agent retained an unacknowledged result. |
| Replacement during recovery | A second operation was queued before the first receipt was retried. Restart resent the identical first receipt, acknowledged it, and left the second operation queued. A subsequent cycle executed the second operation successfully. |
| Independent of policy | Both operations completed without an assigned settings policy; normal reports remained null revision, empty results and null applied revision. |
| Secret isolation | Neither dummy password appeared in durable agent state or normal Console reports. The protocol trace contained only empty claims and non-secret result metadata. |
| Reboot | A changed Windows boot identifier confirmed a real reboot. The acknowledged second operation and its sequence survived, and subsequent claims returned 204. |
| Installer and protected storage | Install/mode upgrade checks and all 85 agent tests passed in the administrator guest, including protected enrollment/state tests omitted on the ordinary development host. |

Relevant commands and requests (credentials/passwords supplied only in dummy lab
request bodies or enrollment stdin):

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked --target x86_64-pc-windows-msvc
powershell.exe -File scripts/install.ps1 -Action Install -SourcePath <agent.exe> -Sha256 <hash> -Mode ObserveOnly
rustdesk-agent.exe --enroll https://192.0.2.11:8444
rustdesk-agent.exe --init-state
powershell.exe -File scripts/install.ps1 -Action Upgrade -SourcePath <agent.exe> -Sha256 <hash> -Mode Managed
Start-Service RustDeskAgent
Stop-Service RustDeskAgent
Restart-Computer -Force
```

Console requests used `POST /api/admin/agent/:id/password`,
`GET /api/admin/agent/:id/password`, and the agent's separate authenticated
`POST /v1/agent/password/claim` and `POST /v1/agent/password/result` routes.

Automated tests additionally cover encrypted payload tampering/wrong-key/device
binding, authorization, expiry and secret erasure, revoked credentials, null-secret
tombstones, storage failure before execution, interrupted execution and duplicate
replay rejection. These are automated tests, not additional VM fault injections.

## Browser and build checks

A separate loopback Console with dummy data rendered the device password form in
Chrome. Submitting matching passwords showed **Password change queued**, cleared
both fields and disabled repeat submission. Normal strategy/report content stayed
visible and unchanged. The only browser resource errors were missing favicon
requests. All 31 UI tests, TypeScript checking and the web build passed; all 18
agent backend tests passed.

On the Windows development host, the broader Console server suite passed 239
tests and failed two unchanged tests: Unix path formatting in `record_storage`
and directory-sync access in `geo_relay::mmdb`. Hosted Linux CI is the full
Console workspace gate; these failures are not reported as a passing local suite.

## Boundaries and cleanup

Both guest checkpoints were restored. Independent verification confirmed both
VMs Off, zero adapters, no test checkpoints and removal of the private switch.
No production environment was contacted or changed.

The integrated service run checks command acceptance and recovery; the separate
manual connection run proves official-command replacement and reboot persistence.
The product does not automatically verify connections or conceal command arguments
from administrators/SYSTEM/privileged monitoring. Physical power loss, other
RustDesk versions and other service accounts remain outside this evidence.
