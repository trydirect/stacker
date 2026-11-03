# Security Policy

Stacker is a deployment tool that runs commands against user-owned servers and cloud
accounts, generates production Docker Compose configurations, and manages secrets. We
treat security seriously because the blast radius of a Stacker vulnerability can be a
user's whole infrastructure.

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.** Public disclosure
before a fix is available exposes users to risk.

Report privately via one of these channels:

- **Email**: [security@try.direct](mailto:security@try.direct) — preferred
- **GitHub Security Advisories**: [Open a draft advisory](https://github.com/trydirect/stacker/security/advisories/new)

If you prefer to encrypt, request our PGP key in your first message and we'll respond
with it before you send technical details.

### What to include

To help us reproduce and prioritise, include as much of the following as you can:

- A clear description of the vulnerability and its potential impact
- Steps to reproduce (a proof-of-concept `stacker.yml`, command sequence, or minimal
  test case is ideal)
- The affected version (`stacker --version`) and, if relevant, target platform
- Whether the issue requires authentication, specific config, or specific timing
- Suggested remediation or mitigation if you have one

### What to expect from us

- **Acknowledgement within 5 business days** of receiving your report
- An assessment of severity and scope, shared back with you
- Regular updates (at least weekly) while we work on a fix
- Credit in the release notes for the fix, unless you prefer to remain anonymous
- Coordinated disclosure: we aim to publish a fix and a security advisory within
  90 days of the initial report, and will discuss the exact timing with you

We are a small team and cannot promise a firm fix SLA, but critical issues are
prioritised over feature work.

### Reports written by AI

If you use an AI assistant to help draft your report, that is fine — please just
verify the vulnerability actually reproduces before submitting. Reports that describe
non-issues (a false positive from a scanner, a general threat pattern with no
specific instance in Stacker) will be closed with a short note. Please do not submit
speculative "AI CVE farming" reports — they consume volunteer time and slow down real
disclosures.

## Supported Versions

Stacker is pre-1.0. Security fixes are released against **the latest tagged
release** on the `main` branch. Users on older versions should upgrade to receive
security fixes; we do not backport to previous minor versions.

| Version | Supported |
| ------- | --------- |
| Latest release (`main`) | ✅ Yes |
| Previous releases | ❌ No |

## Scope

### In scope

The following are considered security issues we want to hear about:

- **Stacker CLI binary** — command injection, path traversal, deserialisation flaws,
  credential leakage in logs or generated files
- **Casbin policies** (`access_control.conf`) — privilege escalation, RBAC bypass,
  role confusion
- **Server-side components** (Status Panel agent, deployment engine) — remote code
  execution, unauthorised container control, secret exfiltration
- **Generated deployment configuration** — cases where Stacker generates a
  `docker-compose.yml` or Nginx config that exposes services more broadly than the
  `stacker.yml` intends (e.g. missing bind-to-localhost on database ports, missing
  reverse proxy on internal services)
- **Vault / secrets integration** — plaintext exposure, insecure defaults,
  cross-tenant leakage in the managed platform
- **Template repositories** (`awesome-selfhosted-stacker`) — templates that ship with
  weak defaults, insecure environment variables, or hardcoded credentials
- **Supply chain** — dependencies pulling in known-vulnerable code, malicious
  releases we've included

### Out of scope

The following are not Stacker vulnerabilities and should be reported to the
upstream project or handled by the operator:

- Vulnerabilities in third-party container images that templates deploy (report to
  the image maintainer — Ghost, Nextcloud, Postgres, etc.)
- Vulnerabilities in Docker, Docker Compose, Nginx Proxy Manager, Traefik, or the
  underlying Linux distribution
- Misconfiguration in user-provided `stacker.yml` (e.g. deliberately exposing a
  database port publicly)
- Attacks that require pre-existing root access to the target server, physical
  access, or social engineering of the user
- DoS via resource exhaustion on user-owned infrastructure (this is expected under
  adversarial load)
- Reports based solely on the output of a scanner without a demonstrated exploit
  path

If you are unsure whether something is in scope, err on the side of reporting.
We would rather triage a false positive than miss a real issue.

## Our Security Practices

Stacker's codebase includes a dedicated security test suite covering the
categories most likely to introduce vulnerabilities in a deployment tool. As of
this policy the suite includes:

- IDOR (Insecure Direct Object Reference) checks across deployments, projects,
  admin actions, ratings, and cloud resources
- Command injection tests against CLI-facing surfaces
- Authorization boundary tests against the agent, chat, server, and pipe interfaces
- SSH key handling and cloud credential isolation tests

See [`tests/security_*.rs`](https://github.com/trydirect/stacker/tree/main/tests)
for the current set. New surfaces that touch authorisation or secrets require a
security test as part of the PR.

## Recognition

We publish a thanks section in release notes for reporters of valid
vulnerabilities, unless the reporter prefers to remain anonymous. If you would
like your GitHub handle, a personal site, or a company affiliation included,
mention it in your report.

We do not currently run a paid bounty program. That may change as the project
grows.

## PGP / Encryption

Available on request. Include a note in your initial email asking for our
public key and we will respond before you send technical details.

---

**Contact**: [security@try.direct](mailto:security@try.direct)
**Advisories**: <https://github.com/trydirect/stacker/security/advisories>
**License**: This project is [MIT-licensed](./LICENSE); this security policy applies
to Stacker as distributed in this repository.
