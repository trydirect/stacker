# Hetzner Cloud KVM Guide for Kata Containers

## Why Dedicated-CPU Servers?

Kata Containers run each container workload inside a lightweight virtual machine
using KVM (Kernel-based Virtual Machine). This requires direct access to the
`/dev/kvm` device, which is only available on servers with dedicated CPU
resources.

On Hetzner Cloud, this means you **must** use CCX-series server types.

## CCX Server Types

The CCX line provides dedicated vCPUs — your workload gets exclusive access to
physical CPU cores, and the hypervisor exposes `/dev/kvm` to the guest OS.

| Type | vCPU | RAM | Disk | Monthly Cost (approx.) | Kata Ready |
|---|---|---|---|---|---|
| CCX13 | 2 | 8 GB | 80 GB | ~€14 | ✅ |
| CCX23 | 4 | 16 GB | 160 GB | ~€29 | ✅ |
| CCX33 | 8 | 32 GB | 240 GB | ~€57 | ✅ |
| CCX43 | 16 | 64 GB | 360 GB | ~€113 | ✅ |
| CCX53 | 32 | 128 GB | 600 GB | ~€225 | ✅ |
| CCX63 | 48 | 192 GB | 960 GB | ~€337 | ✅ |

> Prices are approximate and vary by datacenter location. Check
> [hetzner.com/cloud](https://www.hetzner.com/cloud#pricing) for current pricing.

## Why Shared-CPU Types Don't Work

Shared-CPU types (CX, CPX, CAX) run on a hypervisor that does **not** expose
`/dev/kvm` to guests. Without KVM, the Kata hypervisor cannot create hardware-
isolated VMs, and `kata-runtime` will fail with:

```
kata-runtime: arch requires KVM to run, but /dev/kvm is not accessible
```

There is no workaround — nested virtualisation is not supported on Hetzner
shared-CPU instances.

## Verifying KVM Access

After provisioning a CCX server, verify KVM is available:

```bash
# Check /dev/kvm exists
ls -la /dev/kvm
# Expected: crw-rw---- 1 root kvm 10, 232 ... /dev/kvm

# Check KVM module is loaded
lsmod | grep kvm
# Expected: kvm_intel (or kvm_amd) and kvm modules

# Run Kata's own validation
kata-runtime check
# Expected: all checks pass
```

## Provisioning a Kata-Ready CCX Server

### Option 1: TFA Terraform Module

```bash
cd tfa/terraform/htz/kata

# Initialize
tofu init

# Review the plan
tofu plan \
  -var="hcloud_token=$HCLOUD_TOKEN" \
  -var="hcloud_ssh_key=my-key" \
  -var="server_type=ccx13" \
  -var="datacenter_location=fsn1"

# Apply
tofu apply \
  -var="hcloud_token=$HCLOUD_TOKEN" \
  -var="hcloud_ssh_key=my-key"
```

The module provisions a CCX13 by default with:
- Ubuntu 22.04
- Docker CE pre-installed
- Kata Containers pre-installed
- `daemon.json` configured with `kata` runtime
- Firewall with SSH, HTTP, HTTPS

### Option 2: Manual Setup on Existing CCX Server

```bash
# SSH into your CCX server
ssh root@<server-ip>

# Verify KVM (should exist on CCX)
ls -la /dev/kvm

# Install Kata (Ubuntu 22.04+)
curl -fsSL https://packages.kata-containers.io/kata-containers.key \
  | gpg --dearmor -o /etc/apt/keyrings/kata-containers.gpg
echo "deb [signed-by=/etc/apt/keyrings/kata-containers.gpg] \
  https://packages.kata-containers.io/stable/ubuntu/$(lsb_release -cs)/ \
  stable main" > /etc/apt/sources.list.d/kata-containers.list
apt-get update && apt-get install -y kata-containers

# Configure Docker
cat /etc/docker/daemon.json | python3 -c "
import sys, json
d = json.load(sys.stdin) if sys.stdin.read() else {}
d.setdefault('runtimes', {})['kata'] = {'path': '/usr/bin/kata-runtime'}
json.dump(d, sys.stdout, indent=2)
" | tee /tmp/daemon.json && mv /tmp/daemon.json /etc/docker/daemon.json
systemctl restart docker

# Test
docker run --rm --runtime kata hello-world
```

### Option 3: Hetzner Robot (Bare-Metal)

For production workloads requiring maximum performance, Hetzner Robot dedicated
servers provide direct hardware access. KVM is always available on bare-metal.
Use the TFA Ansible `kata_containers` role to configure these servers.

## Network Considerations

See [NETWORK_CONSTRAINTS.md](NETWORK_CONSTRAINTS.md) for important networking
limitations when running Kata containers, particularly around `network_mode: host`.

## Performance Notes

Running containers inside Kata VMs adds overhead compared to `runc`:

| Aspect | Overhead |
|---|---|
| Container start time | +0.5–2s (VM boot) |
| Memory | +~30 MB per container (VM overhead) |
| Network latency | +50–150 µs per packet |
| Disk I/O | ~5–10% throughput reduction |
| CPU | Negligible for compute; slight overhead for syscall-heavy workloads |

For web services, APIs, and databases, the overhead is typically negligible.
For latency-critical workloads, benchmark before committing to Kata.

## Troubleshooting

### `/dev/kvm` not found
- Ensure you're using a CCX server type, not CX/CPX/CAX
- Check the server hasn't been migrated to a shared host

### `kata-runtime check` fails
- Run `kata-runtime check --verbose` for detailed diagnostics
- Verify kernel modules: `lsmod | grep kvm`
- Check CPU flags: `grep -c vmx /proc/cpuinfo` (Intel) or `grep -c svm /proc/cpuinfo` (AMD)

### Container fails to start with Kata
- Check Docker logs: `journalctl -u docker -f`
- Check for `network_mode: host` conflicts (not supported)
- Ensure enough memory for VM overhead (~30 MB per container)
