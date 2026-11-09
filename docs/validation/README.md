# Validation archive

These are dated records of specific binaries and environments. They summarize
tested behaviors, results and limits; detailed transcripts remain in raw logs.
A record's remaining-gates section describes that run, not the current project backlog. Use the
[current plan](../plan.md) and [Windows usage](../windows-service.md) for current guidance.

| Record | Scope and follow-up |
| --- | --- |
| [Host attempt — November 8](2026-11-08-host-build-tests.md) | Builds and non-elevated tests on the physical host; no service tests. Superseded for runtime status by the next record. |
| [Isolated service VM — November 8](2026-11-08-service-lifecycle.md) | SCM/HTTPS, ACL isolation, initialization, retries and reboot. RustDesk was absent. |
| [Installed RustDesk 1.4.7 — November 8](2026-11-08-rustdesk-1.4.7-getters.md) | Blank getters and cancellation of a real CLI child. Later diagnosis explains the blank ID/defaults. |
| [Getter diagnosis and GUI — November 8](2026-11-08-getter-diagnosis-and-gui.md) | Missing-NIC ID diagnosis and GUI defaults alongside blank CLI settings. Later setter tests cover non-default values. |
| [Official setter — November 9](2026-11-09-official-cli-setters.md) | Actual agent setters as administrator/LocalService, persistence, restoration and hosted CI. Independent live connection verification remains outstanding. |
| [Setter modes, deadline and connections — November 9](2026-11-09-setter-modes-and-timeout.md) | Remaining values under both accounts, locked GUI comparison, real setter timeout, and successful click-approved and temporary-password connections on a private two-VM network. |

The [unavailable-process test](2026-11-09-unavailable-processes.md) subsequently
confirmed account-dependent offline setter/readback behavior and restoration.
The [concurrent-change test](2026-11-09-concurrent-settings.md) preserved the other
allowed key sequentially but observed detected lost updates in 4/20 overlapping
rounds, followed by restoration.
The [serialized-setter rerun](2026-11-09-serialized-setters.md) retained both
changes in 20/20 rounds with the new CLI lock and verified bounded busy handling.
The [setter child-cleanup test](2026-11-09-setter-child-cleanup.md) reproduced a
child surviving agent termination and verified the process-containment fix.
The [managed application test](2026-11-09-managed-application.md) exercised the
default-off loopback service path, drift, partial results, retries and cancellation.
The [managed crash/reboot test](2026-11-09-managed-crash-recovery.md) then verified
actual setter cleanup after forced service death, retained uncertainty, identical
report retries through reboot and resumption only after fresh desired state.
The [managed connection test](2026-11-09-managed-connections.md) verified click
approval, temporary-password access and rejection after a higher revision selected
permanent-only mode, while service reports remained explicitly unverified.

All runtime work used disposable isolated Windows VMs and dummy credentials.
The [credential rotation test](credential-rotation.md) passed administrator
replacement, interrupted handoff, restart and revocation recovery, plus
ordinary-user CLI replacement rejection without changing protected files.
These records do not authorize production deployment or automatic service writes.
