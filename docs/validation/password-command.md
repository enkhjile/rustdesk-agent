# Official password command

## Scope

Official RustDesk 1.4.7 on Windows x64, running the password command as
`NT AUTHORITY\LocalService`. The installed executable was
`C:\Program Files\RustDesk\rustdesk.exe`, SHA-256
`7A43F30721826E6D6B11686A47B3F6D672DC58C1BDB2B62076D704E8BA5235C0`.
The guest reports Windows build 26200. The command probe was a temporary Windows
service with its service SID enabled using `sc.exe sidtype <probe> unrestricted`.

Two disposable VMs used a private Hyper-V switch, addresses `192.0.2.10` and
`192.0.2.11`, and no default route or host/Internet uplink. Passwords were dummy
lab values. This validates the official command, not a completed Console password
action or agent password-delivery implementation.

## Password replacement and persistence

Commands invoked the official executable directly, without a shell or client
modifications:

```text
rustdesk.exe --version
rustdesk.exe --option approve-mode password
rustdesk.exe --option verification-method use-permanent-password
rustdesk.exe --password <dummy-password>
```

Direct-IP access was temporarily enabled in the target guest with
`rustdesk.exe --option direct-server Y`. Its firewall allowed port 21118 only
from the Peer VM. The agent service was stopped and disabled during this command
probe, so normal policy polling could not change the test settings.

| Check | Result |
| --- | --- |
| Set password A as LocalService | Exit 0, `Done!`, empty stderr. |
| Connect from Peer using A | Target desktop opened; operator confirmed password entry and the Peer session was visually inspected. |
| Replace A with B as LocalService | Exit 0, `Done!`, empty stderr. |
| Use previously working A on a fresh connection | Rejected with the incorrect-password dialog, visually confirmed. |
| Retry with B | Target desktop opened, visually confirmed. |
| Set B, reboot target, then make a fresh connection with B | Target desktop opened. A changed Windows boot identifier was checked before reconnecting; the password command was not rerun after reboot. |

Password remembrance was left unchecked. The reboot check used a separate restored
lab run; it establishes persistence of B, not an additional post-reboot rejection
test for A. No product runtime verification badge is justified by these manual
acceptance tests. `Done!` alone remains command acceptance, not authentication
proof.

## Process access

A separate isolated probe created suspended official RustDesk children with a
dummy `--password` argument, allowing deterministic access checks before the
children were terminated without executing. It compared explicit process/thread
permissions with default permissions inherited from a LocalService Windows
service whose service SID was enabled. Both configurations passed the tested
denials; the default service process itself was also checked.

| Caller | Process query/read and permission changes | Thread context | WMI command line |
| --- | --- | --- | --- |
| Probe's LocalService service identity | Allowed for its protected child | Allowed | Visible |
| Unrelated LocalService service | Denied for protected child, default child and parent service | Denied | Not returned |
| Ordinary local user, not an administrator | Denied | Denied | Not returned |
| Elevated LabAdmin | Allowed | Allowed | Visible |

The ordinary-user probe ran under a temporary local account through the service
manager. It recorded its identity and non-administrator status. Process access
checks requested `PROCESS_QUERY_LIMITED_INFORMATION`,
`PROCESS_QUERY_INFORMATION | PROCESS_VM_READ`, and `WRITE_DAC`; the denied
OpenProcess calls returned access denied. WMI could enumerate the process while
withholding its command line.

The command still takes the secret in an argument. These results do not hide it
from administrators, SYSTEM, privileged process-creation auditing, EDR or crash
dumps. No claim is made that every Windows telemetry configuration avoids
recording arguments. Other RustDesk versions and service-account configurations
need their own validation.

## Restoration and remaining work

Both VM checkpoints were restored after the tests. Final independent inspection
confirmed both guests Off, zero network adapters, no remaining test checkpoints,
and removal of the private test switch. No production credentials or settings
were used or changed.

The feature branches add authenticated per-device operations, encrypted bounded
server retention, secret-free durable execution metadata, duplicate/crash handling
and UI controls. This record covers the official command only; integrated delivery
results are recorded separately. Product outcomes distinguish command acceptance
from an unknown outcome and do not claim independent live verification.
