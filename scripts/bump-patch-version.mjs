#!/usr/bin/env node
/**
 * Bump patch version across package.json, src-tauri/Cargo.toml, and
 * src-tauri/tauri.conf.json. Prints the new version to stdout.
 *
 * Usage: node scripts/bump-patch-version.mjs
 */
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

const packagePath = join(root, "package.json");
const cargoPath = join(root, "src-tauri", "Cargo.toml");
const tauriConfPath = join(root, "src-tauri", "tauri.conf.json");

const pkg = JSON.parse(readFileSync(packagePath, "utf8"));
const current = String(pkg.version || "").trim();
const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(current);
if (!match) {
  console.error(`Invalid version in package.json: ${current}`);
  process.exit(1);
}

const next = `${match[1]}.${match[2]}.${Number(match[3]) + 1}`;

pkg.version = next;
writeFileSync(packagePath, `${JSON.stringify(pkg, null, 2)}\n`);

let cargo = readFileSync(cargoPath, "utf8");
const cargoReplaced = cargo.replace(
  /^version\s*=\s*"[^"]+"/m,
  `version = "${next}"`
);
if (cargoReplaced === cargo) {
  console.error("Could not find version in src-tauri/Cargo.toml");
  process.exit(1);
}
writeFileSync(cargoPath, cargoReplaced);

const tauriConf = JSON.parse(readFileSync(tauriConfPath, "utf8"));
tauriConf.version = next;
writeFileSync(tauriConfPath, `${JSON.stringify(tauriConf, null, 2)}\n`);

process.stdout.write(next);
