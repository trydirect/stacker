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

## The step that is easy to miss

**An existing redirect will win over the Worker.** `get.stacker.my`
currently answers `301 -> http://stacker.my`, and in Cloudflare's order of
operations Single Redirects and Page Rule forwarding run *before* Workers.
If that rule is left in place the Worker never sees the request and nothing
appears to change.

Remove or scope that redirect before, or immediately after, deploying. The
symptom if it is still there is simple: the check below keeps returning
`301`.

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
