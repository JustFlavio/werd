#!/usr/bin/env node
// Regenerates catalog/catalog.json from official sources.
//
//   node scripts/catalog/generate.mjs              # every product
//   node scripts/catalog/generate.mjs php node     # only these products
//
// Hashes already present in the catalog are reused for identical URLs, so only
// new releases are downloaded. When a provider fails, its previous entry is kept.
// `generated` changes only when the products change, so unchanged runs produce no diff.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { hashUrl } from "./lib.mjs";
import { providers } from "./providers.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const target = join(root, "catalog", "catalog.json");
const previous = existsSync(target) ? JSON.parse(readFileSync(target, "utf8")) : { schema: 1, products: {} };

const knownHashes = new Map();
for (const product of Object.values(previous.products)) {
  for (const line of Object.values(product.lines)) {
    for (const build of Object.values(line.builds)) {
      for (const part of [build, ...(build.extra ?? [])]) knownHashes.set(part.url, part.sha256);
    }
  }
}

const ctx = {
  known: (url) => knownHashes.has(url),
  async sha256(url, upstream) {
    if (upstream) {
      if (!/^[0-9a-f]{64}$/.test(upstream)) throw new Error(`Bad upstream sha256 for ${url}`);
      return upstream;
    }
    if (!knownHashes.has(url)) knownHashes.set(url, await hashUrl(url));
    return knownHashes.get(url);
  },
};

const selected = process.argv.slice(2);
// Products without a provider (renamed or removed) are dropped.
const products = Object.fromEntries(Object.entries(previous.products).filter(([id]) => id in providers));
let failures = 0;
for (const [id, provider] of Object.entries(providers)) {
  if (selected.length && !selected.includes(id)) continue;
  console.log(`${id}…`);
  try {
    products[id] = await provider(ctx);
    console.log(`  ${Object.keys(products[id].lines).join(", ")}`);
  } catch (error) {
    failures++;
    console.warn(`  failed, keeping previous entry: ${error.message}`);
  }
}

/** Recursively sorts object keys so the output is stable across runs. */
function sorted(value) {
  if (Array.isArray(value)) return value.map(sorted);
  if (value === null || typeof value !== "object") return value;
  const keys = Object.keys(value).sort((a, b) => a.localeCompare(b, undefined, { numeric: true }));
  return Object.fromEntries(keys.map((name) => [name, sorted(value[name])]));
}

const nextProducts = sorted(products);
const changed = JSON.stringify(nextProducts) !== JSON.stringify(sorted(previous.products));
const catalog = {
  schema: 1,
  generated: changed || !previous.generated ? new Date().toISOString() : previous.generated,
  products: nextProducts,
};

mkdirSync(dirname(target), { recursive: true });
writeFileSync(target, `${JSON.stringify(catalog, null, 2)}\n`);
console.log(changed ? `catalog updated (${target})` : "catalog unchanged");
if (failures) process.exitCode = 1;
