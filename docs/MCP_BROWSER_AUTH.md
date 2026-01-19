# MCP Browser-Based Authentication Enhancement

## Current Status

✅ **Backend works perfectly** with `Authorization: Bearer <token>` for server-side clients  
❌ **Backend doesn't support** browser-based clients (cookie authentication needed)

The Stacker MCP WebSocket endpoint (`/mcp`) currently supports:
- ✅ **Bearer Token via Authorization header** (works for server-side clients)
- ❌ **Cookie-based authentication** (needed for browser clients)

**Both methods should coexist** - Bearer for servers, cookies for browsers.

## The Browser WebSocket Limitation

Browser JavaScript WebSocket API **cannot set custom headers** like `Authorization: Bearer <token>`. This is a **W3C specification limitation**, not a backend bug.

### Current Working Configuration

**✅ Server-side MCP clients work perfectly:**
- CLI tools (wscat, custom tools)
- Desktop applications
- Node.js, Python, Rust clients
- Any non-browser WebSocket client

**Example - Works Today:**
```bash
wscat -c "ws://localhost:8000/mcp" \
  -H "Authorization: Bearer 52Hq6LCh16bIPjHkzQq7WyHz50SUQc"
# ✅ Connects successfully
```

### What Doesn't Work

**❌ Browser-based JavaScript:**
```javascript
// Browser WebSocket API - CANNOT set Authorization header
const ws = new WebSocket('ws://localhost:8000/mcp', {
    headers: { 'Authorization': 'Bearer token' }  // ❌ Ignored by browser!
});
// Result: 403 Forbidden (no auth token sent)
```

**Why browsers fail:**
1. W3C WebSocket spec doesn't allow custom headers from JavaScript
2. Browser security model prevents header manipulation
3. Only cookies, URL params, or subprotocols can be sent

## Solution: Add Cookie Authentication as Alternative

**Goal**: Support **BOTH** auth methods:
- Keep Bearer token auth for server-side clients ✅
- Add cookie auth for browser clients ✅

### Implementation

**1. Create Cookie Authentication Method**

Create `src/middleware/authentication/method/f_cookie.rs`:

```rust
use crate::configuration::Settings;
use crate::middleware::authentication::get_header;
use crate::models;
use actix_web::{dev::ServiceRequest, web, HttpMessage, http::header::COOKIE};
use std::sync::Arc;

pub async fn try_cookie(req: &mut ServiceRequest) -> Result<bool, String> {
    // Get Cookie header
    let cookie_header = get_header::<String>(&req, "cookie")?;
    if cookie_header.is_none() {
        return Ok(false);
    }

    // Parse cookies to find access_token
    let cookies = cookie_header.unwrap();
    let token = cookies
        .split(';')
        .find_map(|cookie| {
            let parts: Vec<&str> = cookie.trim().splitn(2, '=').collect();
            if parts.len() == 2 && parts[0] == "access_token" {
                Some(parts[1].to_string())
            } else {
                None
            }
        });

    if token.is_none() {
        return Ok(false);
    }

    // Use same OAuth validation as Bearer token
    let settings = req.app_data::<web::Data<Settings>>().unwrap();
    let user = super::f_oauth::fetch_user(settings.auth_url.as_str(), &token.unwrap())
        .await
        .map_err(|err| format!("{err}"))?;

    tracing::debug!("ACL check for role (cookie auth): {}", user.role.clone());
    let acl_vals = actix_casbin_auth::CasbinVals {
        subject: user.role.clone(),
        domain: None,
    };

    if req.extensions_mut().insert(Arc::new(user)).is_some() {
        return Err("user already logged".to_string());
    }

    if req.extensions_mut().insert(acl_vals).is_some() {
        return Err("Something wrong with access control".to_string());
    }

    Ok(true)
}
```

**Key Points:**
- ✅ Cookie auth uses **same validation** as Bearer token (reuses `fetch_user`)
- ✅ Extracts `access_token` from Cookie header
- ✅ Falls back gracefully if cookie not present (returns `Ok(false)`)

**2. Update Authentication Manager to Try Cookie After Bearer**

Edit `src/middleware/authentication/manager_middleware.rs`:

```rust
fn call(&self, mut req: ServiceRequest) -> Self::Future {
    let service = self.service.clone();
    async move {
        let _ = method::try_agent(&mut req).await?
            || method::try_oauth(&mut req).await?
            || method::try_cookie(&mut req).await?  // Add this line
```

**Authentication Priority Order:**
1. Agent authentication (X-Agent-ID header)
2. **Bearer token** (Authorization: Bearer ...) ← Server clients use this
3. **Cookie** (Cookie: access_token=...) ← Browser clients use this
4. HMAC (stacker-id + stacker-hash headers)
5. Anonymous (fallback)
        Ok(req)
    }
    // ... rest of implementation
}
```

**3. Export Cookie Method**

Update `src/middleware/authentication/method/mod.rs`:

```rust
pub mod f_oauth;
pub mod f_cookie;  // Add this
pub mod f_hmac;
pub mod f_agent;
pub mod f_anonym;

pub use f_oauth::*;
pub use f_cookie::*;  // Add this
pub use f_hmac::*;
pub use f_agent::*;
pub use f_anonym::*;
```

### Browser Client Benefits

Once cookie auth is implemented, browser clients work automatically with **zero code changes**:

```javascript
// Browser automatically sends cookies with WebSocket handshake
const ws = new WebSocket('ws://localhost:8000/mcp');

ws.onopen = () => {
    console.log('Connected! Cookie sent automatically by browser');
    // Cookie: access_token=... was sent in handshake
    
    // Send MCP initialize request
    ws.send(JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: {
            protocolVersion: "2024-11-05",
            clientInfo: { name: "Browser MCP Client", version: "1.0.0" }
        }
    }));
};

ws.onmessage = (event) => {
    const response = JSON.parse(event.data);
    console.log('**NOT** set (JavaScript needs to read token for HTTP API calls)
3. **Secure**: Set to `true` in production (HTTPS only)
4. **Domain**: Match your application domain
5. **Path**: Set to `/` to include WebSocket endpoint

**Example cookie configuration:**
```javascript
// When user logs in, set cookie
document.cookie = `access_token=${token}; path=/; SameSite=Lax; max-age=86400`;
```

## Current Workaround (Server-Side Clients Only)

Until cookie auth is added, use server-side MCP clients that support Authorization headers:

**Node.js (Server-Side) No Auth (Should Still Work as Anonymous)**
```bash
wscat -c "ws://localhost:8000/mcp"

# Expected: Connection successful, limited anonymous permissions
**Test Cookie Authentication:**
```bash
# Set cookie and connect
wscat -c "ws://localhost:8000/mcp" \
  -H "Cookie: access_token=52Hq6LCh16bIPjHkzQq7WyHz50SUQc"
```

**Browser Console Test:**
```javascript
// Set cookie
document.cookie = "access_token=YOUR_TOKEN_HERE; path=/; SameSite=Lax";

// Connect (cookie sent automatically)
const ws = new WebSocket('ws://localhost:8000/mcp');
```

## Current Workaround (Server-Side Only)

For now, use server-side MCP clients that support Authorization headers:

**Node.js:**
```javascript
const WebSocket = require('ws');
const ws = new WebSocket('ws://localhost:8000/mcp', {
    headers: { 'Authorization': 'Bearer YOUR_TOKEN' }
});
```

**Python:**
```python
import websockets

async with websockets.connect(
    'ws://localhost:8000/mcp',
    extra_headers={'Authorization': 'Bearer YOUR_TOKEN'}
) as ws:
    # ... MCP protocol
```

## Priority

**Low Prior Assessment

**Implementation Priority: MEDIUM** 

**Implement cookie auth if:**
- ✅ Building browser-based MCP client UI
- ✅ Creating web dashboard for MCP management
- ✅ Developing browser extension for MCP
- ✅ Want browser-based AI Assistant feature

**Skip if:**
- ❌ MCP clients are only CLI tools or desktop apps
- ❌ Using only programmatic/server-to-server connections
- ❌ No browser-based UI requirements

## Implementation Checklist

- [ ] Create `src/middleware/authentication/method/f_cookie.rs`
- [ ] Update `src/middleware/authentication/manager_middleware.rs` to call `try_cookie()`
- [ ] Export cookie method in `src/middleware/authentication/method/mod.rs`
- [ ] Test with `wscat` using `-H "Cookie: access_token=..."`
- [ ] Test with browser WebSocket connection
- [ ] Verify Bearer token auth still works (backward compatibility)
- [ ] Update Casbin ACL rules if needed (cookie auth should use same role as Bearer)
- [ ] Add integration tests for cookie auth

## Benefits of This Approach

✅ **Backward Compatible**: Existing server-side clients continue working  
✅ **Browser Support**: Enables browser-based MCP clients  
✅ **Same Validation**: Reuses existing OAuth token validation  
✅ **Minimal Code**: Just adds cookie extraction fallback  
✅ **Secure**: Uses same security model as REST API  
✅ **Standard Practice**: Cookie auth is standard for browser WebSocket

- [src/middleware/authentication/manager_middleware.rs](../src/middleware/authentication/manager_middleware.rs)
- [src/middleware/authentication/method/f_oauth.rs](../src/middleware/authentication/method/f_oauth.rs)
- [src/mcp/websocket.rs](../src/mcp/websocket.rs)
