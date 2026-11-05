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
} finally {
  server.kill();
}

if (failed > 0) {
  process.exit(1);
}
console.log("serving contract OK");
