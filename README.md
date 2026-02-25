<div align="center">

<a href="https://discord.gg/mNhsa8VdYX"><img alt="Discord" src="https://img.shields.io/discord/578119430391988232?label=discord"></a>
<img alt="Version" src="https://img.shields.io/badge/version-0.2.3-blue">
<img alt="License" src="https://img.shields.io/badge/license-MIT-green">

<br><br>
<img width="300" src="https://repository-images.githubusercontent.com/448846514/3468f301-0ba6-4b61-9bf1-164c06c06b08">

**Build, deploy, and manage containerised applications with a single config file.**

</div>

 install.shStacker is a CLI + server platform that turns any project into a deployable Docker stack. Add a `stacker.yml` to your repo, and Stacker generates Dockerfiles, docker-compose definitions, reverse-proxy configs, and deploys locally or to cloud providers — optionally with AI assistance.

---

## Quick Start

### Install

```bash
curl -fsSL https://raw.githubusercontent.com/trydirect/stacker/main/install.sh | bash
```

### Create a project

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

## What `stacker.yml` looks like

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

See the full schema reference in [docs/STACKER_YML_REFERENCE.md](docs/STACKER_YML_REFERENCE.md).

---

## CLI Commands

| Command | Description |
|---------|-------------|
| `stacker init` | Detect project type, generate `stacker.yml` + `.stacker/` artifacts |
| `stacker deploy` | Build & deploy the stack (local, cloud, or server) |
| `stacker status` | Show running containers and health |
| `stacker logs` | View container logs (`--follow`, `--service`, `--tail`) |
| `stacker destroy` | Tear down the deployed stack |
| `stacker config validate` | Validate `stacker.yml` syntax |
| `stacker config show` | Show resolved configuration |
| `stacker config example` | Print a full commented `stacker.yml` reference |
| `stacker config setup cloud` | Guided cloud deployment setup |
| `stacker ai ask "question"` | Ask the AI about your stack |
| `stacker proxy add` | Add a reverse-proxy domain entry |
| `stacker proxy detect` | Auto-detect existing reverse-proxy containers |
| `stacker login` | Authenticate with the TryDirect platform |
| `stacker update` | Check for updates and self-update |

### Deploy targets

```bash
# Local (default) — docker compose up
stacker deploy --target local

# Cloud — provisions infrastructure via Terraform + Ansible
stacker deploy --target cloud

# Existing server — deploys via SSH
stacker deploy --target server

# Dry run — show what would be generated without executing
stacker deploy --dry-run
```

---

## Features

- **Auto-detection** — identifies Node, Python, Rust, Go, PHP, static sites from project files
- **Dockerfile generation** — produces optimised multi-stage Dockerfiles per app type
- **Docker Compose generation** — wires app + services + proxy + monitoring into a single compose
- **AI-assisted config** — scans project context, calls LLM to generate tailored `stacker.yml`
- **AI troubleshooting** — on deploy failures, suggests fixes using AI or deterministic fallback hints
- **Reverse proxy** — auto-detects Nginx / Nginx Proxy Manager, configures domains + SSL
- **Cloud deployment** — Hetzner, DigitalOcean, AWS, Linode via Terraform + Ansible
- **Status Panel agent** — deployed alongside your app for real-time monitoring, logs, and remote commands
- **MCP Server** — 48+ Model Context Protocol tools for AI-agent-driven infrastructure management
- **Vault integration** — secrets and config stored in HashiCorp Vault, synced to deployments
- **Marketplace** — publish and deploy community stacks via TryDirect platform

---

## Server (for platform operators)

Stacker also runs as a web server powering the Stack Builder UI, API, and deployment orchestration.

### Setup

```bash
cp configuration.yaml.dist configuration.yaml   # edit database, vault, AMQP settings
cp access_control.conf.dist access_control.conf
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/stacker
sqlx migrate run
cargo run --bin server
```

Server runs on `http://127.0.0.1:8000` by default.

### Key API endpoints

| Endpoint | Description |
|----------|-------------|
| `POST /project` | Create a project from a stack definition |
| `POST /{id}/deploy/{cloud_id}` | Deploy to a cloud provider |
| `GET /project/{id}/apps` | List apps in a project |
| `PUT /project/{id}/apps/{code}/env` | Update app environment variables |
| `POST /api/v1/commands` | Enqueue a command for the Status Panel agent |
| `GET /api/v1/agent/commands/wait/{hash}` | Agent long-polls for commands (HMAC-authenticated) |
| `POST /api/v1/agent/commands/report` | Agent reports command results |

### Database migrations

```bash
sqlx migrate run      # apply
sqlx migrate revert   # rollback
```

### Testing

```bash
cargo test                         # all tests (467+)
cargo test user_service_client     # User Service connector
cargo test marketplace_webhook     # Marketplace webhook flows
cargo test deployment_validator    # Deployment validation
```

---

## Architecture

```
stacker CLI (stacker.yml) ──► Dockerfile + docker-compose.yml ──► docker compose up
                          └──► Stacker Server API ──► Terraform/Ansible ──► Cloud
                                    │
                          Status Panel Agent ◄──── pull-only command queue
                                    │
                              HashiCorp Vault (secrets & config)
```

- **CLI binary** (`src/bin/stacker.rs`) — end-user tool, no server needed for local deploys
- **Server binary** (`src/main.rs`) — REST API, Stack Builder UI, deployment orchestration
- **Console binary** (`src/console/main.rs`) — admin tools (agent token rotation, Casbin debug, etc.)

---

## Documentation

- [stacker.yml reference](docs/STACKER_YML_REFERENCE.md) — full configuration schema
- [CLI implementation plan](docs/STACKER_CLI_PLAN.md) — architecture and design decisions
- [Changelog](CHANGELOG.md) — release history

---

## License

[MIT](LICENSE)
