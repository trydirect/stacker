# stacker.yml Configuration Reference

> **Stacker CLI v0.2** — The single-file deployment configuration for containerised applications.

`stacker.yml` is the only file you need to add to your project. Stacker reads it to auto-generate Dockerfiles, docker-compose definitions, and deploy your application locally or to cloud infrastructure.

---

## Table of Contents

- [Quick Start](#quick-start)
- [Minimal Example](#minimal-example)
- [Full Example](#full-example)
- [Top-Level Fields](#top-level-fields)
  - [name](#name) · [version](#version) · [organization](#organization)
- [app — Application Source](#app)
  - [type](#apptype) · [path](#apppath) · [dockerfile](#appdockerfile) · [image](#appimage) · [build](#appbuild)
- [services — Sidecar Containers](#services)
- [proxy — Reverse Proxy](#proxy)
  - [type](#proxytype) · [auto_detect](#proxyauto_detect) · [domains](#proxydomains) · [config](#proxyconfig)
- [deploy — Deployment Target](#deploy)
  - [target](#deploytarget) · [compose_file](#deploycompose_file) · [cloud](#deploycloud) · [server](#deployserver)
- [ai — AI Assistant](#ai)
- [monitoring — Health & Metrics](#monitoring)
  - [status_panel](#monitoringstatus_panel) · [healthcheck](#monitoringhealthcheck) · [metrics](#monitoringmetrics)
- [hooks — Lifecycle Scripts](#hooks)
- [env / env_file — Environment Variables](#env--env_file)
- [Environment Variable Interpolation](#environment-variable-interpolation)
- [Auto-Detection](#auto-detection)
- [Generated Dockerfiles](#generated-dockerfiles)
- [Validation Rules](#validation-rules)
- [CLI Commands Reference](#cli-commands-reference)
- [Recipes](#recipes)
- [FAQ](#faq)

---

## Quick Start

```bash
# 1. Install stacker
curl -fsSL https://stacker.try.direct/install.sh | bash

# 2. Initialize in your project directory
cd my-project
stacker init

# 3. Review the generated config
cat stacker.yml

# 4. Deploy locally
stacker deploy --target local

# 5. Check status
stacker status
```

---

## Minimal Example

The smallest valid `stacker.yml`:

```yaml
name: my-app
app:
  type: static
  path: ./public
deploy:
  target: local
```

This tells Stacker to:
1. Generate an nginx-based Dockerfile serving static files from `./public`
2. Create a docker-compose.yml with the app service
3. Deploy locally via `docker compose up`

---

## Full Example

A production-ready configuration using all available sections:

```yaml
name: my-saas-app
version: "2.0"
organization: acme-corp

app:
  type: node
  path: ./src
  build:
    context: .
    args:
      NODE_ENV: production

services:
  - name: postgres
    image: postgres:16
    ports:
      - "5432:5432"
    environment:
      POSTGRES_DB: myapp
      POSTGRES_USER: app
      POSTGRES_PASSWORD: ${DB_PASSWORD}
    volumes:
      - pgdata:/var/lib/postgresql/data

  - name: redis
    image: redis:7-alpine
    ports:
      - "6379:6379"

  - name: worker
    image: myapp-worker:latest
    depends_on:
      - postgres
      - redis
    environment:
      REDIS_URL: redis://redis:6379

proxy:
  type: nginx
  auto_detect: true
  domains:
    - domain: app.example.com
      ssl: auto
      upstream: app:3000
    - domain: api.example.com
      ssl: auto
      upstream: app:3000

deploy:
  target: cloud
  cloud:
    provider: hetzner
    region: fsn1
    size: cx21
    ssh_key: ~/.ssh/id_ed25519

ai:
  enabled: true
  provider: ollama
  model: llama3
  endpoint: http://localhost:11434
  timeout: 600
  tasks:
    - dockerfile
    - troubleshoot

monitoring:
  status_panel: true
  healthcheck:
    endpoint: /health
    interval: 30s
  metrics:
    enabled: true
    telegraf: true

hooks:
  pre_build: ./scripts/pre-build.sh
  post_deploy: ./scripts/post-deploy.sh
  on_failure: ./scripts/notify-failure.sh

env_file: .env

env:
  APP_PORT: "3000"
  LOG_LEVEL: info
  NODE_ENV: production
```

---

## Top-Level Fields

### `name`

**Required** · `string` · Max 128 characters

The project name. Used as the docker-compose project name, container name prefix, and displayed in status output.

```yaml
name: my-awesome-app
```

### `version`

*Optional* · `string` · Default: none

A version label for the configuration. Informational only — does not affect behaviour.

```yaml
version: "1.0"
```

### `organization`

*Optional* · `string` · Default: none

Organisation slug. Used for scoping cloud deployments and linking to your TryDirect account.

```yaml
organization: acme-corp
```

---

## `app`

**Application source configuration.** Tells Stacker what kind of app you're building and where the source code lives.

### `app.type`

*Optional* · `enum` · Default: `static`

The application framework/runtime. Determines which Dockerfile template is generated.

| Value | Description | Default Base Image | Default Port |
|-------|-------------|-------------------|--------------|
| `static` | Static HTML/CSS/JS site | `nginx:alpine` | 80 |
| `node` | Node.js application | `node:20-alpine` | 3000 |
| `python` | Python application | `python:3.12-slim` | 8000 |
| `rust` | Rust application | `rust:1.77-alpine` | 8080 |
| `go` | Go application | `golang:1.22-alpine` | 8080 |
| `php` | PHP application | `php:8.3-fpm-alpine` | 9000 |
| `custom` | User-provided Dockerfile | — | — |

```yaml
app:
  type: node
```

> **Tip:** If you omit `type`, Stacker auto-detects it from your project files.
> See [Auto-Detection](#auto-detection).

### `app.path`

*Optional* · `string` (path) · Default: `.`

Path to the application source directory, relative to the `stacker.yml` location.

```yaml
app:
  path: ./src
```

### `app.dockerfile`

*Optional* · `string` (path) · Default: none

Path to a custom Dockerfile. When set, Stacker uses your Dockerfile instead of generating one. Requires `type: custom` or will override the generated template.

```yaml
app:
  type: custom
  dockerfile: ./docker/Dockerfile.prod
```

### `app.image`

*Optional* · `string` · Default: none

Use a pre-built Docker image instead of building from source. Mutually exclusive with `dockerfile` and auto-generation.

```yaml
app:
  type: custom
  image: ghcr.io/myorg/myapp:latest
```

### `app.build`

*Optional* · `object` · Default: none

Docker build configuration. Controls the build context and build arguments passed to `docker build`.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `context` | `string` | `.` | Build context directory |
| `args` | `map<string, string>` | `{}` | Build arguments (`--build-arg`) |

```yaml
app:
  type: node
  build:
    context: .
    args:
      NODE_ENV: production
      API_URL: https://api.example.com
```

---

## `services`

*Optional* · `array` · Default: `[]`

Additional containers deployed alongside your main application — databases, caches, message queues, workers, etc. Each entry maps directly to a service in the generated `docker-compose.yml`.

### Service Definition Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `name` | `string` | **yes** | — | Service name (used as container/hostname) |
| `image` | `string` | **yes** | — | Docker image reference |
| `ports` | `string[]` | no | `[]` | Port mappings (`"host:container"`) |
| `environment` | `map<string, string>` | no | `{}` | Environment variables |
| `volumes` | `string[]` | no | `[]` | Volume mounts (`"name:/path"` or `"./host:/container"`) |
| `depends_on` | `string[]` | no | `[]` | Services this depends on (started first) |

```yaml
services:
  - name: postgres
    image: postgres:16
    ports:
      - "5432:5432"
    environment:
      POSTGRES_DB: myapp
      POSTGRES_PASSWORD: ${DB_PASSWORD}
    volumes:
      - pgdata:/var/lib/postgresql/data

  - name: redis
    image: redis:7-alpine
    ports:
      - "6379:6379"

  - name: minio
    image: minio/minio:latest
    ports:
      - "9000:9000"
      - "9001:9001"
    environment:
      MINIO_ROOT_USER: admin
      MINIO_ROOT_PASSWORD: ${MINIO_PASSWORD}
    volumes:
      - minio-data:/data
```

> **Note:** Stacker detects port conflicts across services during validation.
> If two services bind the same host port, you'll get a warning (`W001`).

---

## `proxy`

*Optional* · `object` · Default: `type: none, auto_detect: true`

Reverse proxy configuration. Stacker can auto-detect a running proxy or generate configuration for one.

### `proxy.type`

*Optional* · `enum` · Default: `none`

| Value | Description |
|-------|-------------|
| `nginx` | Standard Nginx reverse proxy |
| `nginx-proxy-manager` | Nginx Proxy Manager (NPM) with web UI |
| `traefik` | Traefik reverse proxy with auto-discovery |
| `none` | No proxy configured |

```yaml
proxy:
  type: nginx
```

### `proxy.auto_detect`

*Optional* · `bool` · Default: `true`

When enabled, Stacker scans running Docker containers for an existing reverse proxy before deploying. If found, it connects your app to the existing proxy instead of creating a new one.

Detection checks for these container images (in priority order):
1. `jc21/nginx-proxy-manager` / `nginx-proxy-manager` → `nginx-proxy-manager`
2. `traefik` → `traefik`
3. `nginx` → `nginx`

```yaml
proxy:
  auto_detect: false  # Don't look for existing proxies
```

### `proxy.domains`

*Optional* · `array` · Default: `[]`

Domain routing rules. Each entry generates a proxy virtual host configuration.

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `domain` | `string` | **yes** | — | Domain name (e.g. `app.example.com`) |
| `upstream` | `string` | **yes** | — | Backend address (e.g. `app:3000`, `http://web:8080`) |
| `ssl` | `enum` | no | `off` | SSL certificate mode |

**SSL modes:**

| Value | Description |
|-------|-------------|
| `auto` | Automatic certificate provisioning (Let's Encrypt) |
| `manual` | Use manually provided certificates |
| `off` | No SSL (HTTP only) |

```yaml
proxy:
  type: nginx
  domains:
    - domain: app.example.com
      ssl: auto
      upstream: app:3000

    - domain: api.example.com
      ssl: auto
      upstream: app:3000

    - domain: staging.example.com
      ssl: off
      upstream: app:3000
```

### `proxy.config`

*Optional* · `string` (path) · Default: none

Path to a custom proxy configuration file. When set, Stacker uses your config instead of generating one.

```yaml
proxy:
  type: nginx
  config: ./nginx/custom.conf
```

---

## `deploy`

**Deployment target configuration.** Controls where and how your stack is deployed.

### `deploy.target`

*Optional* · `enum` · Default: `local`

| Value | Description |
|-------|-------------|
| `local` | Deploy on the local machine via `docker compose` |
| `cloud` | Provision cloud infrastructure and deploy (requires `deploy.cloud`) |
| `server` | Deploy to an existing remote server via SSH (requires `deploy.server`) |

```yaml
deploy:
  target: local
```

### `deploy.compose_file`

*Optional* · `string` (path) · Default: none

Use a custom docker-compose file instead of the auto-generated one. Stacker will skip generation and use this file directly.

```yaml
deploy:
  target: local
  compose_file: ./docker-compose.prod.yml
```

### `deploy.cloud`

*Required when `target: cloud`* · `object`

Cloud infrastructure provisioning settings. Stacker uses Terraform/Ansible under the hood to create servers and deploy your stack.

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `provider` | `enum` | **yes** | — | Cloud provider |
| `region` | `string` | no | Provider default | Data center region |
| `size` | `string` | no | Provider default | Server size/type |
| `ssh_key` | `string` (path) | no | none | Path to SSH private key |

**Supported cloud providers:**

| Value | Provider | Example Regions | Example Sizes |
|-------|----------|----------------|---------------|
| `hetzner` | Hetzner Cloud | `fsn1`, `nbg1`, `hel1` | `cx21`, `cx31`, `cx41` |
| `digitalocean` | DigitalOcean | `nyc1`, `sfo3`, `ams3` | `s-1vcpu-1gb`, `s-2vcpu-4gb` |
| `aws` | Amazon Web Services | `us-east-1`, `eu-west-1` | `t3.micro`, `t3.small` |
| `linode` | Linode (Akamai) | `us-east`, `eu-west` | `g6-nanode-1`, `g6-standard-2` |
| `vultr` | Vultr | `ewr`, `lhr`, `fra` | `vc2-1c-1gb`, `vc2-2c-4gb` |

```yaml
deploy:
  target: cloud
  cloud:
    provider: hetzner
    region: fsn1
    size: cx21
    ssh_key: ~/.ssh/id_ed25519
```

> **Important:** Cloud deployment requires authentication.
> Run `stacker login` first to store your TryDirect credentials.

### `deploy.server`

*Required when `target: server`* · `object`

Remote server settings for deploying to an existing machine via SSH.

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `host` | `string` | **yes** | — | Server hostname or IP address |
| `user` | `string` | no | `root` | SSH username |
| `ssh_key` | `string` (path) | no | none | Path to SSH private key |
| `port` | `integer` | no | `22` | SSH port |

```yaml
deploy:
  target: server
  server:
    host: 203.0.113.42
    user: deploy
    ssh_key: ~/.ssh/deploy_key
    port: 22
```

---

## `ai`

*Optional* · `object` · Default: `enabled: false`

AI/LLM assistant configuration. When enabled, `stacker ai ask` uses the configured provider to answer questions about your Dockerfile, docker-compose, and deployment.

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `enabled` | `bool` | no | `false` | Enable AI features |
| `provider` | `enum` | no | `openai` | LLM provider |
| `model` | `string` | no | Provider default | Model name |
| `api_key` | `string` | no* | none | API key (supports `${VAR}` syntax) |
| `endpoint` | `string` | no | Provider default | Custom API endpoint URL |
| `timeout` | `integer` | no | `300` | Request timeout in seconds (increase for slow models / weak hardware) |
| `tasks` | `string[]` | no | `[]` | Allowed AI task types |

**Supported providers:**

| Value | Provider | Default Endpoint | Requires API Key |
|-------|----------|-----------------|------------------|
| `openai` | OpenAI | `https://api.openai.com/v1` | Yes |
| `anthropic` | Anthropic | `https://api.anthropic.com/v1` | Yes |
| `ollama` | Ollama (local) | `http://localhost:11434` | No |
| `custom` | Any OpenAI-compatible API | Must specify `endpoint` | Varies |

**Task types** (used for prompt specialisation):
- `dockerfile` — Dockerfile optimisation and generation
- `troubleshoot` — Debugging deployment issues
- `compose` — docker-compose configuration help
- `security` — Security review and hardening

```yaml
# Using OpenAI
ai:
  enabled: true
  provider: openai
  model: gpt-4
  api_key: ${OPENAI_API_KEY}
  tasks:
    - dockerfile
    - troubleshoot

# Using local Ollama
ai:
  enabled: true
  provider: ollama
  model: llama3
  endpoint: http://localhost:11434
  timeout: 600  # 10 minutes for large models on slower hardware

# Using a custom OpenAI-compatible API (e.g. Groq, Together AI)
ai:
  enabled: true
  provider: custom
  model: mixtral-8x7b-32768
  api_key: ${GROQ_API_KEY}
  endpoint: https://api.groq.com/openai/v1
```

---

## `monitoring`

*Optional* · `object` · Default: `status_panel: false`

Monitoring and health check configuration.

### `monitoring.status_panel`

*Optional* · `bool` · Default: `false`

Enable the Stacker status panel — a web UI showing container health, resource usage, and deployment status.

```yaml
monitoring:
  status_panel: true
```

### `monitoring.healthcheck`

*Optional* · `object` · Default: none

Application health check settings.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `endpoint` | `string` | `/health` | HTTP path to probe |
| `interval` | `string` | `30s` | Time between checks |

```yaml
monitoring:
  healthcheck:
    endpoint: /api/health
    interval: 15s
```

### `monitoring.metrics`

*Optional* · `object` · Default: none

Metrics collection settings.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | `bool` | `false` | Enable metrics collection |
| `telegraf` | `bool` | `false` | Deploy Telegraf agent for metrics |

```yaml
monitoring:
  metrics:
    enabled: true
    telegraf: true
```

---

## `hooks`

*Optional* · `object` · Default: none

Lifecycle hook scripts. Stacker runs these at specific points during the build and deploy process.

| Field | Type | Description | When it runs |
|-------|------|-------------|------|
| `pre_build` | `string` (path) | Script to run before Docker build | Before `docker build` |
| `post_deploy` | `string` (path) | Script to run after successful deployment | After `docker compose up` succeeds |
| `on_failure` | `string` (path) | Script to run on deployment failure | When any deploy step fails |

```yaml
hooks:
  pre_build: ./scripts/pre-build.sh
  post_deploy: ./scripts/seed-database.sh
  on_failure: ./scripts/alert-team.sh
```

> Hook scripts must be executable (`chmod +x`).

---

## `env` / `env_file`

### `env`

*Optional* · `map<string, string>` · Default: `{}`

Inline environment variables passed to all containers. Supports `${VAR}` interpolation.

```yaml
env:
  APP_PORT: "3000"
  LOG_LEVEL: info
  DATABASE_URL: postgres://app:${DB_PASSWORD}@postgres:5432/myapp
```

### `env_file`

*Optional* · `string` (path) · Default: none

Path to a `.env` file. Loaded before the config is parsed, so variables defined here can be referenced with `${VAR}` syntax anywhere in `stacker.yml`.

```yaml
env_file: .env
```

Example `.env`:
```
DB_PASSWORD=s3cret
MINIO_PASSWORD=admin123
OPENAI_API_KEY=sk-...
```

---

## Environment Variable Interpolation

Any value in `stacker.yml` can reference environment variables using `${VAR_NAME}` syntax. Variables are resolved from the process environment at parse time.

```yaml
name: ${PROJECT_NAME}
app:
  type: node
services:
  - name: postgres
    image: postgres:${PG_VERSION}
    environment:
      POSTGRES_PASSWORD: ${DB_PASSWORD}
deploy:
  target: cloud
  cloud:
    provider: ${CLOUD_PROVIDER}
ai:
  api_key: ${OPENAI_API_KEY}
```

**Rules:**
- Syntax: `${VARIABLE_NAME}` (curly braces required)
- Undefined variables cause a parse error (fail-fast, no silent empty strings)
- Interpolation happens before YAML parsing
- Works in all string values including paths, URLs, and map values

---

## Auto-Detection

When you run `stacker init` without specifying `--app-type`, Stacker scans your project directory and looks for these marker files:

| Files Found | Detected Type |
|-------------|---------------|
| `package.json` | `node` |
| `requirements.txt`, `Pipfile`, `pyproject.toml`, `setup.py` | `python` |
| `Cargo.toml` | `rust` |
| `go.mod` | `go` |
| `composer.json` | `php` |
| `index.html`, `*.html` | `static` |

Detection priority is top-to-bottom. If none of these files are found, it defaults to `static`.

---

## Generated Dockerfiles

When you run `stacker deploy`, Stacker generates a Dockerfile in `.stacker/Dockerfile` based on `app.type`. Here's what each template produces:

### `static`
```dockerfile
FROM nginx:alpine
COPY . /usr/share/nginx/html
EXPOSE 80
```

### `node`
```dockerfile
FROM node:20-alpine
WORKDIR /app
COPY package*.json ./
RUN npm ci --production
COPY . .
EXPOSE 3000
CMD ["node", "server.js"]
```

### `python`
```dockerfile
FROM python:3.12-slim
WORKDIR /app
COPY requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt
COPY . .
EXPOSE 8000
CMD ["python", "-m", "uvicorn", "main:app", "--host", "0.0.0.0", "--port", "8000"]
```

### `rust`
```dockerfile
FROM rust:1.77-alpine
WORKDIR /app
RUN apk add --no-cache musl-dev
COPY . .
RUN cargo build --release
EXPOSE 8080
CMD ["./target/release/app"]
```

### `go`
```dockerfile
FROM golang:1.22-alpine
WORKDIR /app
COPY go.mod ./
COPY go.sum ./
RUN go mod download
COPY . .
RUN go build -o /app/server .
EXPOSE 8080
CMD ["/app/server"]
```

### `php`
```dockerfile
FROM php:8.3-fpm-alpine
WORKDIR /var/www/html
RUN docker-php-ext-install pdo pdo_mysql
COPY . .
EXPOSE 9000
```

### `custom`
No Dockerfile is generated. You must provide either `app.dockerfile` or `app.image`.

> **Customisation:** To modify the generated Dockerfile, deploy once with `--dry-run`, edit `.stacker/Dockerfile`, then deploy again with `--force-rebuild`.

---

## Validation Rules

Stacker validates your configuration both syntactically (YAML structure) and semantically (cross-field logic). Run `stacker config validate` to check.

### Errors (deployment will fail)

| Code | Rule | Field |
|------|------|-------|
| `E001` | Cloud deployment requires `deploy.cloud.provider` | `deploy.cloud.provider` |
| `E002` | Server deployment requires `deploy.server.host` | `deploy.server.host` |
| `E003` | Custom app type requires `app.image` or `app.dockerfile` | `app` |

### Warnings (deployment may have issues)

| Code | Rule | Field |
|------|------|-------|
| `W001` | Port conflict — multiple services bind the same host port | `services.ports` |

### Example output

```
$ stacker config validate
Configuration issues:
  - [E001] Cloud provider configuration is required for cloud deployment (deploy.cloud.provider)
  - [W001] Port 8080 is used by multiple services: api, worker (services.ports)
```

---

## CLI Commands Reference

| Command | Description |
|---------|-------------|
| `stacker init` | Initialize a new project — generates `stacker.yml` |
| `stacker deploy` | Build and deploy the stack |
| `stacker status` | Show container status |
| `stacker logs` | Show container logs |
| `stacker destroy` | Tear down the stack |
| `stacker config validate` | Validate `stacker.yml` |
| `stacker config show` | Display resolved configuration |
| `stacker login` | Authenticate with TryDirect |
| `stacker ai ask` | Ask the AI assistant a question |
| `stacker proxy add` | Add a reverse-proxy domain entry |
| `stacker proxy detect` | Detect running reverse proxies |
| `stacker update` | Check for CLI updates |

### Common flags

```bash
# Init
stacker init --app-type node --with-proxy --with-ai
stacker init --with-ai --ai-provider ollama --ai-model deepseek-r1

# AI init environment variables (override CLI defaults)
# STACKER_AI_PROVIDER  — AI provider (openai, anthropic, ollama, custom)
# STACKER_AI_MODEL     — Model name
# STACKER_AI_API_KEY   — API key (generic, provider-specific vars also supported)
# STACKER_AI_ENDPOINT  — Custom endpoint URL
# STACKER_AI_TIMEOUT   — Request timeout in seconds (default: 300)
# OPENAI_API_KEY       — OpenAI API key (used when provider is openai)
# ANTHROPIC_API_KEY    — Anthropic API key (used when provider is anthropic)
STACKER_AI_TIMEOUT=900 stacker init --with-ai  # 15 min timeout for slow models

# Deploy
stacker deploy --target local          # Deploy locally
stacker deploy --target cloud          # Deploy to cloud
stacker deploy --target local --dry-run  # Generate files without deploying
stacker deploy --file custom.yml       # Use a custom config file
stacker deploy --force-rebuild         # Force rebuild all containers

# Logs
stacker logs                           # All services
stacker logs --service postgres        # Specific service
stacker logs --follow                  # Stream logs
stacker logs --tail 100                # Last 100 lines
stacker logs --since 1h               # Logs from the last hour

# Status
stacker status                         # Table format
stacker status --json                  # JSON output
stacker status --watch                 # Auto-refresh

# Destroy
stacker destroy --confirm              # Required flag (safety guard)
stacker destroy --confirm --volumes    # Also remove volumes

# Config
stacker config validate                # Check stacker.yml
stacker config validate --file prod.yml
stacker config show                    # Display resolved config

# AI
stacker ai ask "How can I optimise this Dockerfile?"
stacker ai ask "Why is my container crashing?" --context ./logs.txt

# Proxy
stacker proxy add example.com --upstream http://app:3000 --ssl auto
stacker proxy detect

# Update
stacker update                         # Check stable channel
stacker update --channel beta          # Check beta channel
```

---

## Recipes

### Static website
```yaml
name: my-website
app:
  type: static
  path: ./dist
deploy:
  target: local
```

### Node.js API with PostgreSQL
```yaml
name: my-api
app:
  type: node
  path: .
services:
  - name: postgres
    image: postgres:16
    ports:
      - "5432:5432"
    environment:
      POSTGRES_DB: api_db
      POSTGRES_PASSWORD: ${DB_PASSWORD}
    volumes:
      - pgdata:/var/lib/postgresql/data
deploy:
  target: local
env:
  DATABASE_URL: postgres://postgres:${DB_PASSWORD}@postgres:5432/api_db
```

### Python Django with Redis and Nginx
```yaml
name: django-app
app:
  type: python
  path: .
  build:
    args:
      DJANGO_SETTINGS_MODULE: myapp.settings.production
services:
  - name: redis
    image: redis:7-alpine
  - name: celery
    image: django-app:latest
    depends_on:
      - redis
    environment:
      CELERY_BROKER_URL: redis://redis:6379/0
proxy:
  type: nginx
  domains:
    - domain: myapp.example.com
      ssl: auto
      upstream: app:8000
deploy:
  target: cloud
  cloud:
    provider: hetzner
    region: fsn1
    size: cx21
    ssh_key: ~/.ssh/id_ed25519
```

### Rust API deployed to existing server
```yaml
name: rust-api
app:
  type: rust
  path: .
deploy:
  target: server
  server:
    host: api.example.com
    user: deploy
    ssh_key: ~/.ssh/deploy_key
monitoring:
  status_panel: true
  healthcheck:
    endpoint: /api/health
    interval: 15s
```

### Pre-built image (no source)
```yaml
name: wordpress-site
app:
  type: custom
  image: wordpress:6-apache
services:
  - name: mysql
    image: mysql:8
    environment:
      MYSQL_ROOT_PASSWORD: ${MYSQL_ROOT_PASSWORD}
      MYSQL_DATABASE: wordpress
    volumes:
      - db-data:/var/lib/mysql
proxy:
  type: nginx
  domains:
    - domain: blog.example.com
      ssl: auto
      upstream: app:80
deploy:
  target: local
```

### Multi-environment with interpolation
```yaml
name: ${APP_NAME}
version: ${APP_VERSION}
app:
  type: node
  build:
    args:
      NODE_ENV: ${NODE_ENV}
      API_URL: ${API_URL}
services:
  - name: postgres
    image: postgres:${PG_VERSION}
    environment:
      POSTGRES_PASSWORD: ${DB_PASSWORD}
deploy:
  target: ${DEPLOY_TARGET}
```

Run with different environments:
```bash
# Development
APP_NAME=myapp APP_VERSION=dev NODE_ENV=development \
  API_URL=http://localhost:3000 PG_VERSION=16 \
  DB_PASSWORD=devpass DEPLOY_TARGET=local \
  stacker deploy

# Production
APP_NAME=myapp APP_VERSION=1.2.3 NODE_ENV=production \
  API_URL=https://api.example.com PG_VERSION=16 \
  DB_PASSWORD=$PROD_DB_PASSWORD DEPLOY_TARGET=cloud \
  stacker deploy
```

---

## FAQ

**Q: Where are generated files stored?**
A: In the `.stacker/` directory. This includes `Dockerfile`, `docker-compose.yml`, and any proxy configuration. Add `.stacker/` to your `.gitignore`.

**Q: Can I edit the generated Dockerfile?**
A: Yes. Run `stacker deploy --dry-run` to generate it, edit `.stacker/Dockerfile`, then `stacker deploy` to build from your modified version.

**Q: What if I already have a Dockerfile?**
A: Set `app.type: custom` and `app.dockerfile: ./Dockerfile`. Stacker will use yours instead of generating one.

**Q: Do I need Docker installed?**
A: Yes. Stacker requires Docker (with Compose v2) for local deployments. For cloud deployments, Docker is provisioned on the remote server automatically.

**Q: How do I keep secrets out of stacker.yml?**
A: Use environment variable interpolation (`${SECRET_VAR}`) and store actual values in `.env` (referenced via `env_file: .env`). Never commit `.env` to version control.

**Q: Can I use Stacker with an existing docker-compose.yml?**
A: Yes. Set `deploy.compose_file: ./docker-compose.yml` and Stacker will use it directly without generating a new one.

**Q: What cloud providers are supported?**
A: Hetzner, DigitalOcean, AWS, Linode, and Vultr. You must `stacker login` first and have the appropriate API keys configured in your TryDirect account.

---

## File Structure

After `stacker init` and `stacker deploy --dry-run`, your project will look like:

```
my-project/
├── stacker.yml              ← Your configuration (you write this)
├── .stacker/                ← Generated artifacts (auto-created)
│   ├── Dockerfile           ← Generated Dockerfile
│   └── docker-compose.yml   ← Generated compose definition
├── .env                     ← Secrets (optional, gitignored)
├── src/                     ← Your application source
└── scripts/                 ← Hook scripts (optional)
    ├── pre-build.sh
    ├── post-deploy.sh
    └── notify-failure.sh
```

---

*Stacker CLI is part of the [TryDirect](https://try.direct) platform.*
