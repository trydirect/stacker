# Connect your AI agent to TryDirect

TryDirect exposes MCP tools that give an agent capabilities it can't do itself:

- **`resolve_image`** — ground truth about any Docker image (exists? digest,
  size, architectures, tags, official/pinned, grade). Stops the "I'll just use
  `trydirect/redis`" class of hallucination. **No auth — call it for free.**
- **`deploy_ephemeral`** — actually run a `docker-compose.yml` on a throwaway
  cloud box and get back a live URL + logs, auto-torn-down after a TTL. Requires
  a token (it provisions real infra).

There are two ways to reach them.

## A. Zero-signup: the public HTTP endpoint

`resolve_image` is a plain public POST — no account, no MCP client:

```bash
curl -s -X POST https://mcp.try.direct/public/resolve_image \
  -H 'Content-Type: application/json' \
  -d '{"reference":"redis:7-alpine"}'
# -> { "exists": true, "official": true, "pinned": true, "digest": "sha256:…",
#      "size_bytes": 12000000, "architectures": ["amd64","arm64/v8"],
#      "recent_tags": [...], "grade": "A" }
```

Great for quick checks, scripts, or an agent framework that calls HTTP tools
directly.

## B. MCP (auto-discovered by your agent's client)

Point any MCP client at the gateway. Once connected, the client calls
`tools/list` and your model **discovers the tools automatically** from their
descriptions — no plugin, no training. The MCP WebSocket is at
`wss://mcp.try.direct/mcp` (subprotocol `mcp`).

`resolve_image` is anonymous; `deploy_ephemeral` needs
`Authorization: Bearer <TryDirect token>` (get one at
https://try.direct/account/tokens).

### Claude Desktop / Cursor / Windsurf / VS Code (`mcp.json`)
```jsonc
{
  "mcpServers": {
    "trydirect": {
      "url": "wss://mcp.try.direct/mcp",
      "headers": { "Authorization": "Bearer YOUR_TRYDIRECT_TOKEN" }
    }
  }
}
```

### Cline / Roo (settings → MCP servers)
Add a server named `trydirect`, transport WebSocket, URL `wss://mcp.try.direct/mcp`,
header `Authorization: Bearer YOUR_TRYDIRECT_TOKEN`.

### Anonymous (resolve_image only, no token)
Omit the `headers` block — the client still discovers and can call
`resolve_image`; authed tools return a permission error until you add a token.

## What your agent should do with them

- Before writing a compose/Dockerfile, call **`resolve_image`** on each image to
  confirm it exists and is multi-arch — don't guess.
- After generating a stack, call **`deploy_ephemeral`** to actually run it and
  read back the live URL / logs, then self-correct from real errors instead of
  hoping it works.

## Self-hosting

Prefer to run it yourself? The gateway ships as `trydirect/agent-gateway` (see
`AGENT_GATEWAY_SETUP.md`); point your `mcp.json` `url` at your own instance.
