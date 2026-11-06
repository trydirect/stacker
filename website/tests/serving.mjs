#!/usr/bin/env node
// Regression test for the website serving contract.
//
// https://stacker.my/blog must serve the blog index page. The `serve` SPA flag
// (`-s` / `--single`) in the Dockerfile CMD rewrites every unmatched path --
// including /blog and /blog/<article> -- to the root index.html, so the
// landing page is served at every blog URL. This test extracts the exact
// `serve` invocation from the Dockerfile CMD and asserts that the blog pages
// are served from their own files.
//
// Run from website/: npm test

import { readFileSync, existsSync } from "node:fs";
import { spawn } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const PORT = 4173;
const BASE = `http://127.0.0.1:${PORT}`;

function fail(message) {
  console.error(`FAIL: ${message}`);
  process.exit(1);
}

function serveArgsFromDockerfile() {
  const dockerfile = readFileSync(join(root, "Dockerfile"), "utf8");
  const match = dockerfile.match(/^CMD\s+(\[[^\n]*\])\s*$/m);
  if (!match) fail("no JSON-array CMD found in Dockerfile");
  let argv;
  try {
    argv = JSON.parse(match[1]);
  } catch {
    fail(`CMD is not valid JSON: ${match[1]}`);
  }
  if (!argv[0] || !argv[0].endsWith("serve")) {
    fail(`CMD does not invoke serve: ${argv.join(" ")}`);
  }
  if (argv.includes("-s") || argv.includes("--single")) {
    console.error(
      "note: CMD uses serve SPA mode (-s); it rewrites /blog to the root page"
    );
  }
  // Keep every CMD argument except the listen address: routing is
  // port-independent and a fixed test port avoids clashing with a dev server.
  const args = [];
  for (let i = 1; i < argv.length; i++) {
    if (argv[i] === "-l" || argv[i] === "--listen") {
      i++;
      continue;
    }
    args.push(argv[i]);
  }
  args.push("-l", String(PORT));
  return args;
}

const dist = join(root, "dist");
if (!existsSync(join(dist, "index.html"))) {
  fail("dist/index.html missing -- run `npm run build` in website/ first");
}
if (!existsSync(join(dist, "blog", "index.html"))) {
  fail("dist/blog/index.html missing -- run `npm run build` in website/ first");
}

const serveBin = join(root, "node_modules", ".bin", "serve");
if (!existsSync(serveBin)) {
  fail("node_modules/.bin/serve missing -- run `npm ci` in website/ first");
}

const args = serveArgsFromDockerfile();
console.log(`serving dist with: serve ${args.join(" ")}`);

const server = spawn(serveBin, args, { cwd: root, stdio: "ignore" });
server.on("error", (err) => fail(`could not start serve: ${err.message}`));

async function waitForServer() {
  for (let i = 0; i < 100; i++) {
    try {
      await fetch(BASE, { redirect: "manual" });
      return;
    } catch {
      await sleep(100);
    }
  }
  fail(`serve did not start listening on port ${PORT}`);
}

const cases = [
  {
    path: "/",
    title: /<title>Stacker /,
    label: "landing page",
  },
  {
    path: "/blog",
    title: /<title>Blog - Stacker<\/title>/,
    label: "blog index",
    hint: "serve SPA mode (-s) rewrites /blog to the root index.html",
  },
  {
    path: "/blog/ai-startup-hosting.html",
    title: /<title>How to Deploy an AI-Built Startup/,
    label: "blog article",
    hint: "serve SPA mode (-s) rewrites blog article URLs to the root index.html",
  },
];

let failed = 0;
try {
  await waitForServer();
  for (const c of cases) {
    const res = await fetch(BASE + c.path, { redirect: "follow" });
    const body = await res.text();
    if (res.status !== 200) {
      failed++;
      console.error(`FAIL: GET ${c.path} -> HTTP ${res.status} (${c.label})`);
    } else if (!c.title.test(body)) {
      failed++;
      console.error(`FAIL: GET ${c.path} does not serve the ${c.label}`);
      if (c.hint) console.error(`      ${c.hint}`);
    } else {
      console.log(`PASS: GET ${c.path} serves the ${c.label}`);
    }
  }

  // The canonical blog URL is /blog (serve's clean-URL redirects drop the
  // trailing slash), so relative links on that page resolve against /. Every
  // internal link must still reach a served page, and the blog's own links
  // must reach the blog pages.
  const blogRes = await fetch(BASE + "/blog", { redirect: "follow" });
  const blogBody = await blogRes.text();
  const refs = [...blogBody.matchAll(/(?:href|src)="([^"]+)"/g)]
    .map((m) => m[1])
    .filter((h) => !/^(?:https?:|#|mailto:)/.test(h));
  let brokenRefs = 0;
  for (const ref of refs) {
    const url = new URL(ref, blogRes.url);
    const res = await fetch(url, { redirect: "follow" });
    if (res.status !== 200) {
      brokenRefs++;
      console.error(
        `FAIL: blog link ${ref} resolves to ${url.pathname} -> HTTP ${res.status}`
      );
    }
  }
  if (brokenRefs === 0) {
    console.log(`PASS: all ${refs.length} internal links on /blog fetch a served page`);
  } else {
    failed++;
  }

  const articleHref = blogBody.match(
    /<a href="([^"]+)">How to Deploy an AI-Built Startup/
  );
  if (!articleHref) {
    failed++;
    console.error("FAIL: blog index has no article title link");
  } else {
    const res = await fetch(new URL(articleHref[1], blogRes.url), {
      redirect: "follow",
    });
    const body = await res.text();
    if (!/<title>How to Deploy an AI-Built Startup/.test(body)) {
      failed++;
      console.error(
        `FAIL: article title link ${articleHref[1]} does not land on the article`
      );
    } else {
      console.log("PASS: article title link lands on the article");
    }
  }

  const activeNav = blogBody.match(
    /<a ([^>]*class="[^"]*nav__link--active[^"]*"[^>]*)>Blog<\/a>/
  );
  const navHref = activeNav && activeNav[1].match(/href="([^"]+)"/);
  if (!navHref) {
    failed++;
    console.error("FAIL: blog index has no active Blog nav link");
  } else {
    const res = await fetch(new URL(navHref[1], blogRes.url), {
      redirect: "follow",
    });
    const body = await res.text();
    if (!/<title>Blog - Stacker<\/title>/.test(body)) {
      failed++;
      console.error(
        `FAIL: Blog nav link ${navHref[1]} does not land on the blog index`
      );
    } else {
      console.log("PASS: Blog nav link lands on the blog index");
    }
  }

  const cssHref = blogBody.match(/<link rel="stylesheet" href="([^"]+)"/);
  if (!cssHref) {
    failed++;
    console.error("FAIL: blog index references no stylesheet");
  } else if (!/\?v=[0-9a-f]+/.test(cssHref[1])) {
    failed++;
    console.error(
      `FAIL: stylesheet ${cssHref[1]} is not cache-busted; CDN/browser caches would keep serving it across deploys`
    );
  } else {
    const res = await fetch(new URL(cssHref[1], blogRes.url), {
      redirect: "follow",
    });
    const css = await res.text();
    if (res.status !== 200 || !css.includes(".blog-card")) {
      failed++;
      console.error(`FAIL: stylesheet ${cssHref[1]} lacks the blog styles`);
    } else {
      console.log("PASS: stylesheet is cache-busted and defines the blog styles");
    }
  }
} finally {
  server.kill();
}

if (failed > 0) {
  process.exit(1);
}
console.log("serving contract OK");
