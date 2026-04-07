<div align="center">

<a href="https://discord.gg/mNhsa8VdYX"><img alt="Discord" src="https://img.shields.io/discord/578119430391988232?label=discord"></a>
<img alt="Version" src="https://img.shields.io/badge/version-0.2.6-blue">
<img alt="License" src="https://img.shields.io/badge/license-MIT-green">

<br><br>
<img width="300" src="https://repository-images.githubusercontent.com/448846514/3468f301-0ba6-4b61-9bf1-164c06c06b08">

**Build, deploy, and manage containerised applications with a single config file.**

</div>

Stacker is a platform for turning any project into a deployable Docker stack. Add a `stacker.yml` to your repo, and Stacker generates Dockerfiles, docker-compose definitions, reverse-proxy configs, and deploys locally or to cloud providers — optionally with AI assistance.


## Quick Start

### Install the CLI

```bash
curl -fsSL https://raw.githubusercontent.com/trydirect/stacker/main/install.sh | bash
```

### Create & deploy a project

```bash
cd my-project
stacker init              # auto-detects project type, generates stacker.yml
stacker deploy            # builds and runs locally via docker compose
stacker status            # check running containers
```

### AI-powered init (optional)

Stacker can scan your project files and use an LLM to generate a tailored `stacker.yml`:

```bash
# Local AI with Ollama (free, private, default)
stacker init --with-ai

# OpenAI
stacker init --with-ai --ai-provider openai --ai-api-key sk-...

# Anthropic (key from env)
export ANTHROPIC_API_KEY=sk-ant-...
stacker init --with-ai --ai-provider anthropic
```

If the AI provider is unreachable, Stacker falls back to template-based generation automatically.

---

## `stacker.yml` example

```yaml
name: my-app
app:
  type: node
  path: ./src
  ports:
    - "8080:3000"
  environment:
    NODE_ENV: production

services:
  - name: postgres
    image: postgres:16
    environment:
      POSTGRES_DB: myapp
      POSTGRES_PASSWORD: ${DB_PASSWORD}

proxy:
  type: nginx
  auto_detect: true
  domains:
    - domain: app.example.com
      ssl: auto
      upstream: app:3000

deploy:
  target: local    # or: cloud, server

ai:
  enabled: true
  provider: ollama
  model: llama3

monitoring:
  status_panel: true
  healthcheck:
    endpoint: /health
    interval: 30s
```

Full schema reference: [docs/STACKER_YML_REFERENCE.md](docs/STACKER_YML_REFERENCE.md)

---


### Three components

| Component | What it does | Binary |
|-----------|-------------|--------|
| **Stacker CLI** | Developer tool — init, deploy, monitor from the terminal | `stacker-cli` |
| **Stacker Server** | REST API + Stack Builder UI + deployment orchestration + MCP Server | `server` |
| **Status Panel Agent** | Deployed alongside your app on the target server — executes commands, streams logs, reports health | *(separate repo)* |

```
┌──────────────┐         ┌──────────────────┐         ┌─────────────────────┐
│  Stacker CLI │────────►│  Stacker Server  │────────►│  Status Panel Agent │
│              │  REST   │                  │  queue  │  (on target server) │
│  stacker.yml │  API    │  Stack Builder UI│  pull   │                     │
│  init/deploy │         │  48+ MCP tools   │◄────────│  health / logs /    │
│  status/logs │         │  Vault · AMQP    │  HMAC   │  restart / exec /   │
└──────────────┘         └──────────────────┘         │  deploy_app / proxy │
                                │                     └─────────────────────┘
                                ▼
                    Terraform + Ansible ──► Cloud
                    (Hetzner, DO, AWS, Linode)
```

---




## 1. Stacker CLI

The end-user tool. No server required for local deploys.

### Commands

| Command | Description |
|---------|-------------|
| `stacker init` | Detect project type, generate `stacker.yml` + `.stacker/` artifacts |
| `stacker deploy` | Build & deploy the stack (local, cloud, or server). `--runtime kata\|runc` selects container runtime |
| `stacker status` | Show running containers and health |
| `stacker logs` | View container logs (`--follow`, `--service`, `--tail`) |
| `stacker list deployments` | List deployments on the Stacker server |
| `stacker destroy` | Tear down the deployed stack |
| `stacker config validate` | Validate `stacker.yml` syntax |
| `stacker config show` | Show resolved configuration |
| `stacker config example` | Print a full commented reference |
| `stacker config setup cloud` | Guided cloud deployment setup |
| `stacker ai ask "question"` | Ask the AI about your stack |
| `stacker proxy add` | Add a reverse-proxy domain entry |
| `stacker proxy detect` | Auto-detect existing reverse-proxy containers |
| `stacker ssh-key generate` | Generate a new SSH key pair for a server (Vault-backed) |
| `stacker ssh-key show` | Display the public SSH key for a server |
| `stacker ssh-key upload` | Upload an existing SSH key pair for a server |
| `stacker service add` | Add a service from the template catalog to `stacker.yml` |
| `stacker service list` | List available service templates (20+ built-in) |
| `stacker agent health` | Check Status Panel agent connectivity and health |
| `stacker agent status` | Display agent snapshot — containers, versions, uptime |
| `stacker agent logs <app>` | Retrieve container logs from the remote agent |
| `stacker agent restart <app>` | Restart a container via the agent |
| `stacker agent deploy-app` | Deploy or update an app container on the target server. `--runtime kata\|runc` selects container runtime |
| `stacker agent remove-app` | Remove an app container (with optional volume/image cleanup) |
| `stacker agent configure-proxy` | Configure Nginx Proxy Manager via the agent |
| `stacker agent history` | Show recent command execution history |
| `stacker agent exec` | Execute a raw agent command with JSON parameters |
| `stacker submit` | Package current stack and submit to marketplace for review |
| `stacker marketplace status` | Check submission status for your marketplace templates |
| `stacker marketplace logs <name>` | Show review comments and history for a submission |
| `stacker login` | Authenticate with the TryDirect platform |
| `stacker update` | Check for updates and self-update |

### Deploy targets

```bash
stacker deploy --target local     # docker compose up (default)
stacker deploy --target cloud     # Terraform + Ansible → cloud provider
stacker deploy --target server    # deploy to existing server via SSH
stacker deploy --dry-run          # preview generated files without executing
```

### Marketplace workflow (for stack developers)

```bash
stacker deploy --target local           # 1. test locally
stacker deploy --target server          # 2. test on remote server
stacker submit                          # 3. submit for marketplace review
stacker marketplace status              # 4. check review status
# Stack is auto-published once approved by the review team
```

### Marketplace install (for buyers)

```bash
# Option A: Deploy from your laptop to a remote server
stacker deploy my-stack --target server --host 1.2.3.4

# Option B: Run directly on the target server (one-liner)
curl -sL https://marketplace.try.direct/<purchase-token>/install.sh | sh
```

### Key features

- **Auto-detection** — identifies Node, Python, Rust, Go, PHP, static sites from project files
- **Dockerfile generation** — produces optimised multi-stage Dockerfiles per app type
- **Docker Compose generation** — wires app + services + proxy + monitoring
- **AI-assisted config** — scans project, calls LLM to generate tailored `stacker.yml`
- **AI troubleshooting** — on deploy failure, suggests fixes via AI or deterministic fallback hints
- **Service catalog** — 20+ built-in service templates (Postgres, Redis, WordPress, etc.) — add with `stacker service add`
- **AI service addition** — ask `stacker ai ask --write "add wordpress"` and the AI uses the template catalog
- **Agent control** — `stacker agent` subcommand to manage remote Status Panel agents (health, logs, restart, deploy, proxy) with `--json` output
- **SSH key management** — generate, view, and upload server SSH keys (Vault-backed)
- **Reverse proxy** — auto-detects Nginx / Nginx Proxy Manager, configures domains + SSL
- **Cloud deployment** — Hetzner, DigitalOcean, AWS, Linode
- **Marketplace** — submit stacks for review, auto-publish on approval, check status from CLI
- **Buyer install** — purchase tokens, one-liner install scripts, agent self-registration

---

## 2. Stacker Server

The backend platform powering the Stack Builder UI, REST API, deployment orchestration, and MCP server for AI agents.

### Setup

```bash
cp configuration.yaml.dist configuration.yaml   # edit database, vault, AMQP settings
cp access_control.conf.dist access_control.conf
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/stacker
sqlx migrate run
cargo run --bin server                           # http://127.0.0.1:8000
```

### Key API endpoints

| Endpoint | Description |
|----------|-------------|
| `POST /project` | Create a project from a stack definition |
| `POST /{id}/deploy/{cloud_id}` | Deploy to a cloud provider |
| `GET /project/{id}/apps` | List apps in a project |
| `PUT /project/{id}/apps/{code}/env` | Update app environment variables |
| `PUT /project/{id}/apps/{code}/ports` | Update port mappings |
| `PUT /project/{id}/apps/{code}/domain` | Update domain / SSL settings |
| `POST /api/v1/commands` | Enqueue a command for the Status Panel agent |
| `POST /api/templates` | Create or update a marketplace template (creator) |
| `POST /api/templates/{id}/submit` | Submit template for marketplace review |
| `GET /api/templates/mine` | List current user's template submissions |
| `GET /api/v1/marketplace/install/{token}` | Generate install.sh script for buyers |
| `GET /api/v1/marketplace/download/{token}` | Download stack archive (purchase token validated) |
| `POST /api/v1/marketplace/agents/register` | Agent self-registration after install |

### MCP Server

Stacker exposes **52+ Model Context Protocol tools** over WebSocket, enabling AI agents (Claude, GPT, etc.) to manage infrastructure programmatically:

- Project & deployment management
- Container operations (start, stop, restart, exec)
- Log analysis & error summaries
- Vault config read/write
- Proxy configuration
- App environment & port management
- Server resource monitoring
- Docker Compose generation & preview
- Agent control (deploy app, remove app, configure proxy, get status)
- Firewall management (iptables rules via Status Panel or SSH)

### Key integrations

- **HashiCorp Vault** — secrets and config storage, synced to deployments
- **RabbitMQ** — deployment status updates, event-driven orchestration
- **TryDirect User Service** — OAuth, marketplace templates, payment validation
- **Marketplace** — publish and deploy community stacks

---


## 3. Status Panel Agent

A lightweight agent deployed alongside your application on the target server. It runs as a Docker container and communicates with Stacker Server using a **pull-only architecture** — the agent polls for commands, Stacker never dials out.

### How it works

```
1. UI/API creates a command       →  POST /api/v1/commands
2. Command stored in DB queue     →  commands + command_queue tables
3. Agent polls for work           →  GET /api/v1/agent/commands/wait/{hash}
4. Agent executes locally         →  Docker API on the host
5. Agent reports result           →  POST /api/v1/agent/commands/report
```

All agent requests are **HMAC-signed** (`X-Agent-Signature` header) using a token stored in Vault.

### Supported commands

| Command | Description |
|---------|-------------|
| `health` | Check container health status (single or all) |
| `logs` | Fetch container logs (stdout/stderr, with limits) |
| `restart` | Restart a container |
| `deploy_app` | Deploy or update an app container |
| `remove_app` | Remove an app container |
| `configure_proxy` | Create/update/delete reverse-proxy entries |
| `configure_firewall` | Configure iptables firewall rules (add/remove/list/flush) |
| `stacker.exec` | Execute a command inside a running container (with security blocklist) |
| `stacker.server_resources` | Collect server resource metrics (CPU, memory, disk, network) |
| `apply_config` | Pull config from Vault and apply to a running container |

### Agent registration

```bash
# Agent self-registers on first boot (no auth required)
POST /api/v1/agent/register
  { "deployment_hash": "abc123", "capabilities": [...], "system_info": {...} }
  → { "agent_id": "...", "agent_token": "..." }
```

### Token rotation

```bash
cargo run --bin console -- Agent rotate-token \
  --deployment-hash <hash> \
  --new-token <NEW_TOKEN>
```

---

## Database migrations

```bash
sqlx migrate run      # apply
sqlx migrate revert   # rollback
```

## Testing

```bash
cargo test                         # all tests (467+)
cargo test user_service_client     # User Service connector
cargo test marketplace_webhook     # Marketplace webhook flows
cargo test deployment_validator    # Deployment validation
```

---

## Kata Containers (Hardware Isolation)

Stacker supports [Kata Containers](https://katacontainers.io/) as an alternative runtime, providing VM-level isolation for each container using hardware virtualization (KVM).

**KVM requirement** — Kata needs nested or bare-metal KVM. Hetzner dedicated-CPU servers (CCX line) expose `/dev/kvm` out of the box, making them an ideal deployment target.

```bash
stacker deploy --runtime kata          # deploy the current stack with Kata isolation
stacker agent deploy-app --runtime kata  # deploy a single app container with Kata
```

See [docs/kata/](docs/kata/README.md) for the full setup guide, network constraints, and monitoring reference. Automated provisioning (Ansible + Terraform for Hetzner CCX) is available via the TFA infrastructure toolkit.

---

## Documentation

- [stacker.yml reference](docs/STACKER_YML_REFERENCE.md) — full configuration schema
- [CLI implementation plan](docs/STACKER_CLI_PLAN.md) — architecture and design decisions
- [Changelog](CHANGELOG.md) — release history
- [Kata Containers guide](docs/kata/README.md) — hardware-isolated containers with KVM

---

## License

[MIT](LICENSE)
