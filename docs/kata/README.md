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

## Prerequisites

| Requirement | Minimum |
|---|---|
| CPU | x86_64 with VT-x/VT-d **or** aarch64 with virtualization extensions |
| Kernel | Linux 5.4+ with KVM module loaded |
| Docker | 20.10+ |
| Host OS | Ubuntu 22.04+ (playbook-tested) |
| Hardware | Bare-metal or VM with nested virtualisation enabled |

## Contents

| Path | Description |
|---|---|
| [ansible/kata-setup.yml](ansible/kata-setup.yml) | Ansible playbook — installs & configures Kata + Docker on Ubuntu |
| [terraform/](terraform/) | Terraform module — provisions a KVM-capable Hetzner server with Kata pre-installed |
| [NETWORK_CONSTRAINTS.md](NETWORK_CONSTRAINTS.md) | Networking limitations and workarounds when using Kata |

## Quick Start

```bash
# Provision a server (Hetzner)
cd terraform
terraform init && terraform apply

# Configure the server
cd ../ansible
ansible-playbook -i <server-ip>, kata-setup.yml -u root

# Deploy with Stacker
stacker deploy --runtime kata
```

## References

- [Kata Containers documentation](https://github.com/kata-containers/kata-containers/tree/main/docs)
- [Kata with Docker](https://github.com/kata-containers/kata-containers/blob/main/docs/install/docker/ubuntu-docker-install.md)
- [Supported hardware](https://github.com/kata-containers/kata-containers/blob/main/docs/Requirements.md)
