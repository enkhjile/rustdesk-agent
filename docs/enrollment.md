# Local enrollment and credential storage

This is a development implementation, not a deployment approval. It provisions
one device locally; it does not register it with a server, rotate credentials,
or change the service account. Once restarted, the service uses valid enrollment
for read-only management polling; see [service behavior](windows-service.md).

## Storage design

Enrollment generates a random UUID v4 and stores it together with an HTTPS origin
and an administrator-supplied per-device bearer credential. The entire document
is encrypted using Windows DPAPI with machine scope and UI forbidden. It lives
at the Windows Known Folder ProgramData path under
`RustDeskAgentEnrollment\enrollment.dpapi`, separate from diagnostic snapshots.
The location is obtained from Windows, not environment variables. The system
ProgramData parent directory must retain its administrator-managed protections.

Machine-scoped DPAPI lets an administrator provision data for a different service
account without depending on that account's profile. **Any local user who obtains
the blob can decrypt it on this machine.** File permissions are therefore an
essential boundary, not an optional extra. This does not protect against local
administrators, SYSTEM, an already compromised agent or full machine compromise.

The directory and file are created with a protected DACL and administrator owner:
SYSTEM and Administrators have full access; the `NT SERVICE\RustDeskAgent` SID
has read/execute access. No parent permissions are inherited. Loading checks the
owner/DACL against the expected policy and rejects reparse points for both the
directory and file. It does not repair or loosen permissions automatically.
The service can read enrollment but cannot overwrite it through its service SID.

The fixed directory must not already exist during enrollment. Creation is
exclusive and refuses to adopt or overwrite existing contents, including an
incomplete prior attempt. The file is flushed before success is reported.
A crash may leave an incomplete store; subsequent loads fail closed. There is no
automatic recovery, identity replacement or credential rotation. An administrator
must investigate and coordinate any reprovisioning with the backend.

Plaintext buffers owned by the enrollment/storage code use zeroization, and the
DPAPI output buffer is cleared before freeing. This reduces retention; it is not
a guarantee against memory capture or copies made inside OS/serialization code.
Debug output redacts the credential. Error messages do not include input values.

## Provisioning workflow (validated with dummy credentials in the isolated VM)

1. Install the Windows Service as `RustDeskAgent` using an explicitly selected
   account. Enable its service SID with `sc.exe sidtype RustDeskAgent unrestricted`
   and restart it for the SID to be present. This adds the service SID to its
   token; it does not select LocalSystem or grant general administrator rights.
2. In an elevated administrator process, run
   `rustdesk-agent.exe --enroll https://management.example` with the credential
   supplied through **redirected stdin from an approved secret provider**.
   Interactive input is refused to avoid echoing secrets. Never put the credential
   in command arguments, shell literals/history, environment variables or logs.
   A single trailing newline is accepted. Input must be a 32–256-character
   ASCII bearer token using letters, digits or `-._~+/=`. Length validation does
   not prove entropy: use a cryptographically random, unique backend credential.
3. The command prints only UUID and normalized HTTPS origin. Bind that UUID to
   the supplied credential in the management backend through its admin process.
   This repository still has no production enrollment endpoint. Do not use a
   Console administrator API key as the device credential.
4. `rustdesk-agent.exe --enrollment-info` loads and validates the store, printing
   only UUID and origin. The read-only service also loads it at startup and includes
   those public fields in snapshots. Run the administrator-only [state initialization](state.md) once after enrollment and before starting an enrolled service.

Origins must use HTTPS and contain no userinfo, path beyond `/`, query or fragment.
Enrollment itself only stores the endpoint. The service connects after restart;
enrollment success does not establish endpoint reachability or server authorization.
Do not clone enrolled machines: UUID and credential belong to one device.

Missing enrollment leaves the current service in read-only diagnostic mode, with
`enrollment: null`. An existing inaccessible, corrupt or mismatched-policy store
stops service startup with code 1. Authenticated polling requires a
valid enrollment and must not treat a missing store as permission to regenerate it.

## Verification

Portable tests cover origin/token validation, stable serialized identity,
redacted output, UUID version/variant, duplicate fields and malformed records. Windows-only tests cover DPAPI round-trip,
tamper rejection, protected file creation, repeated-enrollment refusal, identity
retention and rejection of a mismatched access policy. The storage test requires
an elevated test runner and uses only random temporary paths and dummy credentials.

Windows x64 builds require the MSVC C++ tools and SDK (including the TLS dependency).
PR #3 passed hosted Windows tests, including DPAPI round-trip and protected-store
creation/load checks. Subsequent runtime checks are recorded in the
[isolated VM record](validation/windows-vm-validation.md).
Service-SID access, unrelated-service denial and unattended reboot passed there;
ordinary-user token access denial and CLI initialization rejection also passed. Do not deploy on compile evidence.

Sources: [DPAPI machine scope and UI flags](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata),
[handle-based security inspection](https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-getsecurityinfo),
[service SID types](https://devblogs.microsoft.com/oldnewthing/20231004-00/?p=108849).
