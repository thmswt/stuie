# Security Policy

STUIE handles cloud storage credentials. This document covers how to
report a vulnerability, what's in scope, and the threat model the
project is designed against.

If you spot something that *might* be a security issue and aren't sure,
err on the side of using the private channel below — we'd rather get a
non-issue privately than miss a real one.

---

## Reporting a vulnerability

**Do not open a public GitHub issue for security reports.**

Use **GitHub Private Vulnerability Reporting**:

> https://github.com/thmswt/stuie/security/advisories/new

If that's unavailable for any reason (e.g., your account can't see the
form), email **me@thomas-weit.de** with `[stuie security]` in the
subject line.

A useful report includes:

- The version of STUIE (output of `stuie --version` or the commit SHA
  you built from).
- Operating system and how STUIE was installed (`cargo install`,
  prebuilt binary, source build).
- A minimal reproduction — config snippet (with credentials redacted),
  steps, observed behaviour.
- Impact: what an attacker who exploits this could do.

### What to expect

| | |
|---|---|
| Acknowledgement of the report | within **5 business days** |
| Initial triage + severity assessment | within **10 business days** |
| Fix or documented mitigation | target **30 days** for high/critical, **90 days** for medium/low |
| Disclosure timeline | coordinated; we'll agree a public disclosure date with the reporter and credit you in the advisory unless you prefer otherwise |

These are best-effort targets for a hobby/OSS project — for anything
genuinely critical we'll move faster. If you don't hear back within the
acknowledgement window, please re-send.

---

## Scope

**In scope** (reports welcome):

- Bugs in the STUIE binary that allow a local user to escalate from
  their own credentials' permissions, exfiltrate data they shouldn't
  have access to, or write where they shouldn't.
- Failures in the read-only-mode enforcement layer (the `ReadOnlyStorage`
  decorator and the GCS service-account scope restriction).
- Credential handling bugs: STUIE writing creds to disk in unexpected
  locations, leaking creds to logs, sending creds to the wrong host.
- Path-traversal, injection, or memory-safety issues in the file/
  upload/download paths.
- Supply-chain issues in our distribution (tampered release artifacts,
  typosquats, malicious dependency we shipped).

**Out of scope:**

- Bugs or misconfigurations in the cloud providers themselves (AWS S3,
  GCS, MinIO). Report those upstream.
- Credentials a user has voluntarily entered into their own
  `config.json` being readable by other processes running as that
  same user — that's the OS's threat boundary, not STUIE's.
- Issues that require an attacker to already have arbitrary code
  execution as the user running STUIE.
- DoS via running out of disk space when downloading large objects, or
  similar resource-exhaustion against your own machine.
- Issues in third-party packagers' (homebrew, AUR, distro repos)
  builds — please report to the packager, with a CC to us if the root
  cause is in STUIE itself.

---

## Threat model

STUIE's job is to give a single user interactive access to object-
storage providers they already have credentials for. The trust boundary
is the **OS user account running STUIE**: anything that user can read
on disk is considered accessible. STUIE does not try to defend a user
against themselves or against other processes running as them.

### Where credentials live

- **Static credentials** (S3 access key + secret, GCS service-account
  JSON path) live in `~/.config/stuie/config.json` in **plaintext**.
  They are not encrypted at rest.
- **Gcloud ADC** credentials are managed by `gcloud` itself (typically
  in `~/.config/gcloud/`) and STUIE only reads them via the standard
  ADC mechanism.
- **MinIO/local-S3** credentials are in the same `config.json`.

### Recommended practices for users

- **Set tight file permissions** on the config:
  ```bash
  chmod 600 ~/.config/stuie/config.json
  ```
  STUIE doesn't enforce this today (planned, see *Future hardening*),
  but it's the single highest-value mitigation for local-multi-user
  systems.
- **Prefer short-lived / federated credentials** over long-lived static
  keys where the provider supports it:
  - AWS: assume an IAM role via SSO, AWS-Vault, or similar — populate
    the `config.json` only when needed and clear it after.
  - GCS: prefer **`GcloudAdc`** mode (no secrets in our config at all)
    over `ServiceAccount` with a JSON key.
- **Use Read-Only Mode** for any provider that points at a production
  bucket. The wrapper is always-on; for GCS service-account mode the
  OAuth scope is also restricted at the wire level. See the
  [README's Read-Only Mode section](README.md#read-only-mode).
- **Apply least-privilege IAM policies** on the credentials you do put
  in the config. Sample policies are tracked under
  [PLAN.md §9 [soon] Minimum IAM policy docs](PLAN.md).

### What STUIE deliberately does not do

- **No telemetry, no analytics, no update checks.** STUIE makes
  outbound network calls **only** to the storage endpoints you
  configure (the S3 endpoint, GCS API, OAuth token endpoints for ADC,
  etc.). It does not phone home, post crash reports, or check for
  newer versions.
- **No credential transmission** to anywhere other than the configured
  provider. The bearer tokens and AWS-SigV4 signatures STUIE generates
  are sent only to the host in your provider config.
- **No history of operations** outside the local `tracing` log file
  (gitignored, see `logs/` in this repo's layout). Logs may include
  bucket and object paths but **never** include credentials, tokens,
  or signed URLs — this is enforced by the structured-logging
  conventions in the code.

### Future hardening (tracked, not yet shipped)

- **OS-keychain integration** (macOS Keychain, Linux Secret Service,
  Windows Credential Manager) so credentials don't sit on disk in
  plaintext at all. Tracked in `PLAN.md` §9 [later].
- **`config.json` permission enforcement** — refuse to load if the
  file is world-readable. Tracked as a follow-up.
- **STS-backed RO mode for S3** — mint a temporary token with a
  `Deny` policy when entering RO mode, rather than relying on the
  wrapper alone. Tracked in the README's *Pending Features*.
- **Signed releases** — `cosign`/`sigstore` signatures on binary
  artifacts and `SHA256SUMS`. Tracked in `PLAN.md` §6 [soon].

---

## Defense-in-depth: how RO mode is actually enforced

STUIE's read-only mode is the most security-relevant feature, so it's
worth being explicit about how it works:

1. **UI keybindings short-circuit.** The destructive keys (`n`, `u`,
   `x`/`Delete`) check the provider's RO flag and refuse before any
   storage call is made.
2. **Wrapper layer.** The storage backend is wrapped in a
   `ReadOnlyStorage` decorator that returns an error for `upload`,
   `create_folder`, and `delete_objects` regardless of how it was
   called. Even a bug in the UI layer bypassing step 1 still hits
   this guard.
3. **Credential-side enforcement (GCS service-account mode only).**
   The OAuth access token is minted with the `devstorage.read_only`
   scope, so even if a hypothetical bug bypassed steps 1 and 2 the
   GCS API would reject the write with HTTP 403.

Steps 1 and 2 cover all providers; step 3 is GCS-service-account only.
S3 and GCS-GcloudAdc rely on steps 1 + 2 plus the user's IAM hygiene.

---

## Changes to this policy

This policy will be updated as the project's security posture evolves.
Material changes (new contact channel, scope changes) will be called
out in `CHANGELOG.md` under `Security`.
