#!/usr/bin/env node
/**
 * Assert every Cargo workspace member declares the MSRV implied by
 * `rust-toolchain.toml`.
 *
 * Source of truth:
 *   1. `rust-toolchain.toml` `channel` (exact pin, e.g. `1.97.0`)
 *   2. Expected package `rust_version` = major.minor of that channel (`1.97`)
 *
 * Reads `cargo metadata --no-deps --format-version 1` from stdin so the check
 * uses Cargo's resolved package metadata rather than grepping Cargo.toml files.
 *
 * Usage (CI):
 *   cargo metadata --no-deps --format-version 1 --locked | node scripts/assert_msrv.mjs
 *
 * Optional local override (debug only; CI must not pass this):
 *   ... | node scripts/assert_msrv.mjs 1.97
 */

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const TOOLCHAIN_PATH = path.join(ROOT, "rust-toolchain.toml");

/**
 * @returns {string} major.minor MSRV, e.g. `1.97`
 */
function expectedMsrvFromToolchain() {
  let text;
  try {
    text = fs.readFileSync(TOOLCHAIN_PATH, "utf8");
  } catch (error) {
    console.error(`error: cannot read ${TOOLCHAIN_PATH}: ${error.message}`);
    process.exit(1);
  }

  const channelMatch = text.match(/^\s*channel\s*=\s*"([^"]+)"/m);
  if (!channelMatch) {
    console.error(
      `error: no channel = "..." entry found in ${path.relative(ROOT, TOOLCHAIN_PATH)}`,
    );
    process.exit(1);
  }

  const channel = channelMatch[1];
  const majorMinor = channel.match(/^(\d+\.\d+)/);
  if (!majorMinor) {
    console.error(
      `error: rust-toolchain channel '${channel}' is not a parseable version (expected X.Y or X.Y.Z)`,
    );
    process.exit(1);
  }

  return majorMinor[1];
}

function main() {
  const expected = process.argv[2] ?? expectedMsrvFromToolchain();
  if (process.argv[2]) {
    console.log(
      `note: using CLI override rust_version=${expected} (CI should omit argv)`,
    );
  }

  const chunks = [];
  process.stdin.setEncoding("utf8");
  process.stdin.on("data", (chunk) => chunks.push(chunk));
  process.stdin.on("end", () => {
    let meta;
    try {
      meta = JSON.parse(chunks.join(""));
    } catch (error) {
      console.error(`error: failed to parse cargo metadata JSON: ${error.message}`);
      process.exit(1);
    }

    const members = new Set(meta.workspace_members ?? []);
    const rows = (meta.packages ?? [])
      .filter((pkg) => members.has(pkg.id))
      .map((pkg) => [pkg.name, pkg.rust_version ?? null])
      .sort((a, b) => a[0].localeCompare(b[0]));

    if (rows.length === 0) {
      console.error("error: no workspace members found in cargo metadata");
      process.exit(1);
    }

    console.log(`Expected rust_version: ${expected}`);
    console.log(
      `  (from ${process.argv[2] ? "CLI argv" : "rust-toolchain.toml channel major.minor"})`,
    );
    let failed = false;
    for (const [name, rustVersion] of rows) {
      const label = rustVersion ?? "<missing>";
      console.log(`  ${name}: ${label}`);
      if (rustVersion !== expected) {
        console.error(
          `error: ${name} rust_version is '${label}', expected '${expected}'`,
        );
        failed = true;
      }
    }

    process.exit(failed ? 1 : 0);
  });
}

main();
