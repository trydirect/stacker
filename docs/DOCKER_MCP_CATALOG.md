# Docker MCP Catalog — submission draft

The Docker MCP Catalog + MCP Toolkit is the highest-fit distribution channel:
it's Docker-native and reaches exactly the agent/devops audience, and our tools
are Docker-shaped. We already build and push `trydirect/agent-gateway` in CI
(`.github/workflows/agent-gateway.yml`), which is the image the catalog lists.

Below is the catalog entry metadata to submit (adapt field names to the current
catalog schema / PR template at github.com/docker/mcp-registry).

## Server entry

```yaml
name: trydirect
title: TryDirect — deploy & image ground truth
description: >
  Give your agent hands and eyes for real infrastructure. resolve_image returns
  ground truth about any Docker image (exists, digest, size, architectures,
  tags, official/pinned, grade) so the agent stops hallucinating image refs;
  deploy_ephemeral runs a docker-compose on a throwaway cloud box and returns a
  live URL + logs, auto-torn-down after a TTL.
category: devops
vendor: TryDirect
homepage: https://try.direct
source: https://github.com/trydirect/stacker
image: trydirect/agent-gateway:latest
transport: websocket
endpoint: /mcp
auth:
  type: bearer            # optional: resolve_image works anonymously
  obtain: https://try.direct/account/tokens
tools:
  - name: resolve_image
    description: >
      Ground truth about a Docker image reference (exists, digest, size,
      architectures, recent tags, official/pinned, quality grade). Public — no
      auth required.
  - name: deploy_ephemeral
    description: >
      Run a docker-compose.yml on a throwaway cloud sandbox and get back a live
      URL, logs and health; auto-teardown after ttl_secs. Safety-gated (no
      privileged / docker.sock / host networking) and quota-limited.
```

## Also submit to

- **Official MCP registry** (github.com/modelcontextprotocol/registry) — same
  metadata, their manifest schema.
- **Community catalogs**: Smithery, mcp.so, PulseMCP, Glama — mostly ingest from
  the official registry or a `server.json`, so keep one canonical manifest.
- **Client marketplaces**: Cursor / Cline / Windsurf MCP directories, and the
  Claude connectors directory — list `wss://mcp.try.direct/mcp` with the config
  snippet from `CONNECT_YOUR_AGENT.md`.

## Positioning line (for every listing)

> Cursor/Claude/Copilot can generate a Dockerfile all day; none of them can prove
> the image exists, run the stack, and hand back a live URL. TryDirect can.

## Submission checklist

- [ ] `trydirect/agent-gateway:latest` published (CI does this on push to main).
- [ ] Public demo: `curl -X POST https://mcp.try.direct/public/resolve_image -d '{"reference":"nginx:latest"}'`.
- [ ] Token onboarding page live at /account/tokens.
- [ ] `CONNECT_YOUR_AGENT.md` published on the site (e.g. /tools/mcp).
- [ ] Canonical `server.json` manifest committed for registry ingestion.
