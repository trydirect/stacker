// get.stacker.my -> the installer in this repository, mirrored, never copied.
//
// People pipe this URL into bash. Two rules follow from that and neither is
// negotiable:
//
//   1. It serves exactly one file. Any other path is a 404, so a typo or a
//      crafted link cannot make this host hand a shell something else.
//   2. If GitHub does not answer with a 200, this answers 502 with a plain
//      message. It never falls through to HTML - that is the bug this
//      replaces, where get.stacker.my returned 52 KB of marketing page and
//      `| sh` ran it.

const SOURCE =
  "https://raw.githubusercontent.com/trydirect/stacker/main/install.sh";

// Long enough that a release does not hammer GitHub, short enough that a fix
// to install.sh is live within the coffee break after merging it.
const CACHE_SECONDS = 300;

export default {
  async fetch(request) {
    const url = new URL(request.url);

    // `/` is served too: the command published on the site and in older posts
    // is `curl -fsSL https://get.stacker.my | sh`, with no path.
    if (url.pathname !== "/" && url.pathname !== "/install.sh") {
      return new Response("Not found. This host serves only /install.sh\n", {
        status: 404,
        headers: { "content-type": "text/plain; charset=utf-8" },
      });
    }

    if (request.method !== "GET" && request.method !== "HEAD") {
      return new Response("Method not allowed\n", {
        status: 405,
        headers: { "content-type": "text/plain; charset=utf-8", allow: "GET, HEAD" },
      });
    }

    const upstream = await fetch(SOURCE, {
      cf: { cacheTtl: CACHE_SECONDS, cacheEverything: true },
      headers: { "user-agent": "get.stacker.my mirror" },
    });

    if (!upstream.ok) {
      return new Response(
        `The installer is temporarily unavailable (upstream ${upstream.status}).\n` +
          `Install from ${SOURCE} instead.\n`,
        { status: 502, headers: { "content-type": "text/plain; charset=utf-8" } },
      );
    }

    return new Response(upstream.body, {
      status: 200,
      headers: {
        "content-type": "text/x-shellscript; charset=utf-8",
        "cache-control": `public, max-age=${CACHE_SECONDS}`,
        "x-mirror-of": SOURCE,
      },
    });
  },
};
