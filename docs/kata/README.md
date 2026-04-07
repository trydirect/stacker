# Kata Containers Support

[Kata Containers](https://katacontainers.io/) run workloads inside lightweight VMs,
providing hardware-level isolation while keeping the container UX. Each container
gets its own kernel, so a guest exploit cannot reach the host.

## How Stacker Uses Kata

When you set `runtime: kata` on a deployment, the Stacker agent:

1. Verifies the target host has `kata-runtime` installed and `/dev/kvm` accessible.
2. Injects `runtime: kata` into the generated `docker-compose.yml` service definitions.
3. Validates compose YAML — warns if `network_mode: host` is detected (unsupported under Kata).
4. Deploys the stack normally via Docker Compose.

On the **Stacker server** side:

1. The `runtime` field is validated (`runc` or `kata`) — unknown values are rejected with HTTP 422.
2. Agent capabilities are checked — if the target agent doesn't report `kata` in its `/capabilities` features, the command is rejected.
3. Runtime preference is persisted in the `deployment` table and optionally in Vault.
4. Org-level runtime policies can enforce Kata for all deployments.

## CLI Usage

```bash
# Deploy with Kata isolation
stacker deploy --runtime kata

# Deploy a single app with Kata
stacker agent deploy-app --app myservice --runtime kata

# Default (runc) — no flag needed
stacker deploy
```

The `--runtime` flag is passed through the agent command payload. If the target
server doesn't support Kata, the command is rejected before reaching the agent.

## Prerequisites

| Requirement | Minimum |
|---|---|
| CPU | x86_64 with VT-x/VT-d **or** aarch64 with virtualisation extensions |
| Kernel | Linux 5.4+ with KVM module loaded |
| Docker | 20.10+ |
| Host OS | Ubuntu 22.04+ (playbook-tested) |
| Hardware | Bare-metal or dedicated-CPU VM with KVM access |

## Hetzner Server Types & KVM Support

Kata Containers require direct access to `/dev/kvm`. On Hetzner Cloud, only
**dedicated-CPU** server types expose KVM to the guest:

| Server Type | CPU | KVM Support | Kata Compatible |
|---|---|---|---|
| **CCX13** | 2 dedicated vCPU, 8 GB RAM | ✅ | ✅ Recommended entry-level |
| **CCX23** | 4 dedicated vCPU, 16 GB RAM | ✅ | ✅ |
| **CCX33** | 8 dedicated vCPU, 32 GB RAM | ✅ | ✅ |
| **CCX43** | 16 dedicated vCPU, 64 GB RAM | ✅ | ✅ |
| **CCX53** | 32 dedicated vCPU, 128 GB RAM | ✅ | ✅ |
| **CCX63** | 48 dedicated vCPU, 192 GB RAM | ✅ | ✅ |
| CX22 / CX32 / CX42 / CX52 | Shared vCPU | ❌ | ❌ No KVM access |
| CPX11 / CPX21 / CPX31 / CPX41 / CPX51 | Shared vCPU (AMD) | ❌ | ❌ No KVM access |
| CAX11 / CAX21 / CAX31 / CAX41 | Shared Arm64 | ❌ | ❌ No KVM access |

> **Important:** Shared-CPU types (CX, CPX, CAX) do not expose `/dev/kvm` and
> **cannot** run Kata Containers. Always use CCX (dedicated-CPU) types.

For bare-metal providers (Hetzner Robot, OVH, Scaleway), KVM is always available
since you have full hardware access.

## Provisioning with TFA

The recommended way to provision Kata-ready servers is via the
[TFA](https://github.com/trydirect/try.direct.stacks) project:

### Terraform (Hetzner)

```bash
cd tfa/terraform/htz/kata
tofu init
tofu plan -var="hcloud_token=YOUR_TOKEN" -var="hcloud_ssh_key=my-key"
tofu apply
```

This provisions a CCX13 (dedicated-CPU) server with Docker and Kata
pre-installed via cloud-init.

### Ansible Role

```bash
# Run the kata_containers role on an existing server
ansible-playbook -i <server-ip>, setup_stack.yml \
  --tags kata_containers \
  --private-key ~/.ssh/id_rsa \
  --user root
```

The `kata_containers` role:
- Validates KVM access (`/dev/kvm`)
- Installs Kata Containers from official APT repo
- Merges `kata` runtime into Docker's `daemon.json`
- Restarts Docker and runs a smoke test

### Standalone (without TFA)

Reference playbook and Terraform files are also available in this directory:

| Path | Description |
|---|---|
| [ansible/kata-setup.yml](ansible/kata-setup.yml) | Standalone Ansible playbook |
| [terraform/](terraform/) | Standalone Terraform module for Hetzner |

## Architecture Flow

```
                    ┌─────────────────────────────┐
                    │  stacker deploy --runtime kata │
                    └──────────────┬──────────────┘
                                   │
                                   ▼
                    ┌──────────────────────────────┐
                    │  Stacker Server               │
                    │  1. Validate runtime value     │
                    │  2. Check agent capabilities   │
                    │  3. Check org policy (Vault)   │
                    │  4. Enqueue command             │
                    └──────────────┬───────────────┘
                                   │
                                   ▼
                    ┌──────────────────────────────┐
                    │  Status Panel Agent            │
                    │  1. Detect /dev/kvm            │
                    │  2. Inject runtime: kata       │
                    │  3. Validate compose YAML      │
                    │  4. docker compose up           │
                    └──────────────────────────────┘
```

## Related Documentation

| Document | Description |
|---|---|
| [HETZNER_KVM_GUIDE.md](HETZNER_KVM_GUIDE.md) | Detailed guide for KVM on Hetzner CCX servers |
| [NETWORK_CONSTRAINTS.md](NETWORK_CONSTRAINTS.md) | Why `network_mode: host` doesn't work with Kata, and alternatives |
| [MONITORING.md](MONITORING.md) | Prometheus metrics, PromQL queries, and dashboard specs for Kata tracking |

## Security Benefits

Kata provides defense-in-depth for multi-tenant and untrusted workloads:

- **Kernel isolation**: Each container has its own guest kernel — host kernel exploits are contained.
- **Hardware boundary**: The VMM (QEMU/Cloud Hypervisor) enforces memory isolation via VT-x/EPT.
- **Syscall filtering**: The guest kernel's syscall surface is independent of the host.
- **Compatible with OCI**: Standard Docker images work without modification.

## References

- [Kata Containers documentation](https://github.com/kata-containers/kata-containers/tree/main/docs)
- [Kata with Docker](https://github.com/kata-containers/kata-containers/blob/main/docs/install/docker/ubuntu-docker-install.md)
- [Supported hardware](https://github.com/kata-containers/kata-containers/blob/main/docs/Requirements.md)
- [Hetzner Cloud server types](https://www.hetzner.com/cloud#pricing)
