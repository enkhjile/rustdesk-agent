# Validation archive

These are dated records of specific binaries and environments. Their observations
are retained, including unsuccessful harness attempts. A record's remaining-gates
section describes that run, not the current project backlog. Use the
[current plan](../plan.md) and [Windows usage](../windows-service.md) for current guidance.

| Record | Scope and follow-up |
| --- | --- |
| [Host attempt — November 8](windows-validation-2026-11-08.md) | Builds and non-elevated tests; VM setup was unfinished at that point. Superseded for runtime status by the next record. |
| [Isolated service VM — November 8](windows-vm-validation.md) | SCM/HTTPS, ACL isolation, initialization, retries and reboot. RustDesk was absent. |
| [Installed RustDesk 1.4.7 — November 8](rustdesk-1.4.7-validation.md) | Blank getters and cancellation of a real CLI child. Later diagnosis explains the blank ID/defaults. |
| [Getter diagnosis and GUI — November 8](rustdesk-getter-diagnosis.md) | Missing-NIC ID diagnosis and GUI defaults alongside blank CLI settings. Later setter tests cover non-default values. |
| [Official setter — November 9](official-setter-validation.md) | Actual agent setters as administrator/LocalService, persistence, restoration and hosted CI. Independent live connection verification remains outstanding. |

All runtime work used a disposable isolated Windows VM and dummy credentials.
These records do not authorize production deployment or automatic service writes.
