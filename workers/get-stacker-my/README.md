# get.stacker.my

Serves `install.sh` from this repository's `main` branch at
`https://get.stacker.my/install.sh`, and at `/` as well, because the command
published in older posts has no path.

It mirrors, it does not copy. The repository stays the only place the
installer is edited, which is the point: this repo already carried two
`install.sh` files that had drifted 37 lines apart, and a third copy sitting
in Cloudflare would be worse.

## Why a Worker and not a redirect rule

A redirect works for `curl -fsSL`, which follows redirects. But then the URL
a person sees, and the one in their shell history, is
`raw.githubusercontent.com`. A Worker keeps `get.stacker.my` as the address
and lets us control the content type and cache.

## Deploying

```bash
cd workers/get-stacker-my
npx wrangler login          # once, in a browser
npx wrangler deploy
```

`wrangler.toml` binds the route `get.stacker.my/*` in the `stacker.my` zone.

## Deployed 2026-11-13

Live. Version `3c104d68-e063-4849-b476-129db14ea99a`, account
`info@optimum-web.com`, route `get.stacker.my/*` in zone `stacker.my`.

There was **no redirect rule to remove** - checked in the dashboard, the
`301 -> http://stacker.my` that this host used to return did not come from
Rules. The Worker took over without touching anything else.

One thing did look broken for about a minute after deploying: the first
request returned a cached `301 Moved Permanently` page as the *body*, with
the Worker's own `content-type: text/x-shellscript` on it. That was
Cloudflare serving the pre-deploy response out of cache. A request with a
cache-buster returned the real script immediately, and the cached copy aged
out on its own. If this happens again after a change, it is cache, not code.

## Checks

```bash
curl -sI https://get.stacker.my/install.sh | head -3
# expect: HTTP/2 200, content-type: text/x-shellscript

diff <(curl -s https://get.stacker.my/install.sh) install.sh && echo identical

curl -s -o /dev/null -w '%{http_code}\n' https://get.stacker.my/anything-else
# expect: 404, not the marketing page
```

The second one is the one that matters: it proves the mirror is a mirror.

## After it is live

One change switches every published command over, and they should move
together rather than one at a time:

- `src/cli/ci_export.rs` - the `INSTALLER_URL` constant
- `install.sh` and `scripts/install.sh` - the usage header and its note
- `website/src/blog/ai-startup-hosting.html` and the built `dist/` copy
- `docs/HOMEBREW_TAP.md`, `README.md`, `DOCKERHUB.md`
- in the other repositories: `awesome-selfhosted-stacker` docs, and the
  blog articles in `blog/content/blog-articles/`

Until the checks above pass, leaving them on the raw GitHub URL is correct:
it works, and a prettier address that 301s to a marketing page is how this
bug happened the first time.
