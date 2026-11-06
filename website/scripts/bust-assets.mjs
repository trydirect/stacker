#!/usr/bin/env node
// Append a content hash to styles.css and main.js references in the built
// HTML so CDN and browser caches can never mask a deploy: the URL changes
// whenever the asset content changes. Runs at the end of `npm run build`.

import { readFileSync, writeFileSync, readdirSync, statSync } from "node:fs";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const dist = join(root, "dist");

function htmlFiles(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const p = join(dir, entry);
    if (statSync(p).isDirectory()) out.push(...htmlFiles(p));
    else if (p.endsWith(".html")) out.push(p);
  }
  return out;
}

for (const asset of ["styles.css", "main.js"]) {
  let bytes;
  try {
    bytes = readFileSync(join(dist, asset));
  } catch {
    continue;
  }
  const hash = createHash("md5").update(bytes).digest("hex").slice(0, 8);
  const escaped = asset.replace(".", "\\.");
  const pattern = new RegExp(
    `(?<![.\\w-])${escaped}(\\?v=[0-9a-f]+)?(?![\\w.-])`,
    "g"
  );
  for (const file of htmlFiles(dist)) {
    const html = readFileSync(file, "utf8");
    const updated = html.replace(pattern, `${asset}?v=${hash}`);
    if (updated !== html) writeFileSync(file, updated);
  }
}
