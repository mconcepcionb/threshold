#!/usr/bin/env node
// The four version strings a release reads must agree before a tag is built.
import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.argv[2] ?? ".";
const read = (path) => readFileSync(join(root, path), "utf8");

const pkg = JSON.parse(read("app/package.json")).version;
const tauri = JSON.parse(read("app/src-tauri/tauri.conf.json")).version;
const cargo = (path) => {
  const match = read(path).match(/^version\s*=\s*"([^"]+)"/m);
  if (!match) throw new Error(`${path} has no package version`);
  return match[1];
};
const appCargo = cargo("app/src-tauri/Cargo.toml");
const coreCargo = cargo("core/Cargo.toml");

const found = { pkg, tauri, appCargo, coreCargo };
const versions = new Set(Object.values(found));
if (versions.size !== 1) {
  console.error("Version mismatch:", found);
  process.exit(1);
}

console.log(`versions agree: ${pkg}`);
