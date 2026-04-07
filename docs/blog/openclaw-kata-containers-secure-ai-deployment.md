---
title: "Deploying OpenClaw with Kata Containers: Hardware-Isolated AI on Your Own Server"
date: 2026-04-07
author: try.direct
tags: [openclaw, kata-containers, security, ai, deployment]
---

# Deploying OpenClaw with Kata Containers: Hardware-Isolated AI on Your Own Server

OpenClaw is a personal AI assistant with a multi-channel gateway — think of it
as a self-hosted AI hub that connects to your tools, documents, and workflows.
Running it on your own infrastructure keeps your data private. Running it inside
Kata Containers adds **hardware-level isolation**, ensuring that even if the AI
workload is compromised, it cannot escape to your host system.

This guide covers two ways to get started — pick whichever fits your workflow.

## Why Kata Containers for AI Workloads?

AI assistants like OpenClaw process sensitive data: your documents, API keys,
conversation history, and workspace files. Standard Docker containers (`runc`)
share the host kernel — a container escape exploit could expose everything on
the host.

Kata Containers solve this by running each container inside a lightweight
virtual machine:

| | runc (standard) | Kata |
|---|---|---|
| **Kernel** | Shared with host | Dedicated guest kernel |
| **Isolation** | Linux namespaces + cgroups | Hardware VM boundary (VT-x/EPT) |
| **Escape impact** | Full host access | Contained in VM |
| **Performance** | Native | ~5% overhead |
| **OCI compatible** | ✅ | ✅ |

For AI workloads that handle private data, the security trade-off is
compelling: you get near-native performance with a hardware isolation boundary
that's orders of magnitude harder to bypass than namespace-based containers.

---

## Path A: Deploy via TryDirect (Easiest)

The fastest way to run OpenClaw with Kata — no Terraform, no Ansible, no
infrastructure to manage. TryDirect handles server provisioning, Kata setup,
and deployment for you.

### 1. Create a TryDirect account

Sign up at [try.direct](https://try.direct) and connect your Hetzner
API token (or use TryDirect's built-in hosting).

### 2. Create your stack

From the dashboard, select **OpenClaw** from the app catalog. Choose
**Kata Containers** as the runtime — TryDirect will automatically provision a
Kata-capable server (Hetzner CCX with dedicated CPU and KVM access).

### 3. Deploy

Click **Deploy**. TryDirect handles everything:
- Provisions a CCX server with KVM support
- Installs Docker and Kata Containers
- Generates the compose file with `runtime: kata`
- Deploys OpenClaw with hardware isolation

You get a running OpenClaw instance with Kata isolation in minutes, accessible
via the URL shown in your dashboard.

### 4. Manage

Use the TryDirect dashboard or the Stacker CLI:

```bash
stacker status                     # container health
stacker logs --service openclaw    # view logs
stacker agent status               # verify runtime: kata
```

> **That's it.** If you don't need full control over the infrastructure,
> TryDirect is the recommended path. Read on only if you prefer to self-host.

---

## Path B: Self-Hosted Setup (Full Control)

If you'd rather manage your own servers, you can provision and configure
everything yourself using the Terraform and Ansible files included in the
[stacker repository](https://github.com/trydirect/stacker).

### What You Need

- A Hetzner Cloud account (or any provider with KVM-capable servers)
- [OpenTofu](https://opentofu.org/) (or Terraform) installed locally
- [Ansible](https://docs.ansible.com/ansible/latest/installation_guide/) installed locally
- The [Stacker CLI](https://github.com/trydirect/stacker) installed

> **Hetzner users:** You need a **CCX-series** server (dedicated CPU).
> Shared-CPU types (CX, CPX, CAX) don't expose `/dev/kvm` and cannot run Kata.
> See the [Hetzner KVM Guide](../kata/HETZNER_KVM_GUIDE.md) for details.

### Step 1: Provision a Kata-Ready Server

The stacker repo includes a ready-to-use Terraform module at
[`docs/kata/terraform/`](../kata/terraform/):

```bash
# Clone the stacker repo (if you haven't already)
git clone https://github.com/trydirect/stacker.git
cd stacker/docs/kata/terraform

# Initialize and apply
tofu init
tofu plan \
  -var="hcloud_token=$HCLOUD_TOKEN" \
  -var="ssh_key_name=my-key" \
  -var="server_type=ccx13" \
  -var="location=fsn1"

tofu apply \
  -var="hcloud_token=$HCLOUD_TOKEN" \
  -var="ssh_key_name=my-key"
```

This provisions a Hetzner CCX13 (dedicated-CPU) server with Docker and Kata
pre-installed via cloud-init. The server is ready for deployments once
cloud-init completes (~3–5 minutes).

### Step 2: Configure with Ansible (optional — for existing servers)

If you already have a server or want idempotent configuration, use the Ansible
playbook at [`docs/kata/ansible/kata-setup.yml`](../kata/ansible/kata-setup.yml):

```bash
cd stacker/docs/kata/ansible

ansible-playbook -i <server-ip>, kata-setup.yml \
  --private-key ~/.ssh/id_rsa \
  --user root
```

The playbook:
- Validates KVM access (`/dev/kvm`)
- Installs Kata Containers from the official APT repository
- Merges the `kata` runtime into Docker's `daemon.json`
- Restarts Docker and runs a smoke test (`docker run --rm --runtime kata hello-world`)

### Step 3: Initialize Your OpenClaw Stack

```bash
mkdir openclaw-stack && cd openclaw-stack

# Initialize a stacker project
stacker init

# Add OpenClaw from the service catalog
stacker service add openclaw
```

This generates a `stacker.yml` with OpenClaw configured:

```yaml
name: openclaw-stack
app:
  type: custom

services:
  - name: openclaw
    image: ghcr.io/openclaw/openclaw:latest
    ports:
      - "18789:18789"
    environment:
      OPENCLAW_GATEWAY_BIND: lan
    volumes:
      - openclaw_config:/home/node/.openclaw
      - openclaw_workspace:/home/node/.openclaw/workspace
```

### Step 4: Deploy with Kata Isolation

```bash
stacker deploy --runtime kata
```

That's it. Stacker will:

1. **Validate** the runtime value (`kata` is accepted, unknown values are rejected)
2. **Check capabilities** — verify the target agent supports Kata
3. **Generate** the compose file with `runtime: kata` on each service
4. **Deploy** via Docker Compose on the target server

Each OpenClaw container now runs inside its own lightweight VM with a dedicated
kernel.

### Step 5: Verify the Deployment

```bash
# Check container status
stacker status

# View logs
stacker logs --service openclaw --follow

# Verify Kata runtime is active
stacker agent status
# Look for "runtime": "kata" in the deployment details
```

On the server, you can also verify directly:

```bash
ssh root@<server-ip>
docker inspect openclaw | grep -i runtime
# Expected: "Runtime": "kata"
```

---

## Why This Matters for OpenClaw Specifically

OpenClaw processes and stores:
- **Your conversations** with AI models
- **API keys** for LLM providers (OpenAI, Anthropic, etc.)
- **Workspace files** that may contain proprietary code or documents
- **Gateway configurations** that bridge multiple communication channels

With standard `runc`, a vulnerability in OpenClaw's Node.js runtime, a
dependency supply-chain attack, or a malicious prompt injection that achieves
code execution would have direct access to the host filesystem and network.

With Kata, that exploit is trapped inside a VM:
- It sees a minimal guest kernel, not your host
- It cannot access host files outside its mounted volumes
- It cannot inspect other containers or host processes
- Network access is mediated through a virtual NIC

## Advanced: Mixed Runtime Stacks

Not every service in your stack needs Kata. You can run security-sensitive
services (like OpenClaw) with Kata while keeping supporting services (like
databases) on standard `runc`:

```yaml
services:
  - name: openclaw
    image: ghcr.io/openclaw/openclaw:latest
    runtime: kata          # Hardware-isolated
    ports:
      - "18789:18789"
    environment:
      OPENCLAW_GATEWAY_BIND: lan
    volumes:
      - openclaw_config:/home/node/.openclaw
      - openclaw_workspace:/home/node/.openclaw/workspace

  - name: postgres
    image: postgres:16
    # runtime: runc (default) — database stays on runc for performance
    environment:
      POSTGRES_DB: openclaw
      POSTGRES_PASSWORD: ${DB_PASSWORD}
    volumes:
      - pgdata:/var/lib/postgresql/data
```

This gives you the best of both worlds: hardware isolation where it matters,
native performance where it doesn't.

## Kata Fallback Behavior

If you request `--runtime kata` but the agent detects that Kata is unavailable
(e.g., `/dev/kvm` missing after a host migration), the agent will:

1. Log a `kata_fallback` warning
2. Fall back to `runc`
3. Report the fallback in the deployment result

Stacker surfaces this warning in CLI output:

```
⚠ Warning: Kata runtime unavailable on target host, fell back to runc.
  Reason: /dev/kvm not accessible
```

This ensures your deployment succeeds even if Kata becomes temporarily
unavailable, while keeping you informed about the security downgrade.

## Performance Expectations

Running OpenClaw with Kata vs runc:

| Metric | runc | Kata | Difference |
|---|---|---|---|
| Container start | ~1s | ~2.5s | +1.5s (one-time) |
| Memory overhead | — | ~30 MB | VM baseline |
| HTTP latency (p99) | 2ms | 2.1ms | Negligible |
| LLM API calls | N/A | N/A | Not affected (outbound HTTPS) |
| Workspace file I/O | Native | ~95% | Minimal virtio overhead |

For an AI assistant workload, the overhead is effectively invisible. The extra
1.5 seconds at startup and ~30 MB of memory are trivial compared to the
security benefits.

## Summary

| Path | What you do | What's handled for you |
|---|---|---|
| **TryDirect** | Sign up, pick OpenClaw + Kata, click Deploy | Server, KVM, Docker, Kata, DNS |
| **Self-hosted** | Run `tofu apply` + `stacker deploy --runtime kata` | Compose generation, runtime injection |

Running OpenClaw inside Kata Containers gives you:
- **Privacy**: Your AI data stays on your server, not in a cloud SaaS
- **Isolation**: Hardware-enforced VM boundary around each container
- **Simplicity**: One flag (`--runtime kata`) — everything else is standard Docker
- **Compatibility**: Standard OCI images, no rebuilds required

---

*For more details, see the [Kata Containers documentation](../kata/README.md),
[Hetzner KVM Guide](../kata/HETZNER_KVM_GUIDE.md), and
[Network Constraints](../kata/NETWORK_CONSTRAINTS.md).*