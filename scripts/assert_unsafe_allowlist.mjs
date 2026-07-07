#!/usr/bin/env node
/**
 * Fail if `allow`/`expect(unsafe_code)` appears outside the approved FFI allowlist.
 *
 * Workspace lint is `unsafe_code = "deny"` (not forbid) so FFI modules can
 * suppress it with a reasoned attribute. Both `allow` and `expect` suppress the
 * lint; this script gates either form.
 *
 * Scans first-party Rust sources under crates/ and apps/. Multi-line attributes
 * are matched after collapsing whitespace.
 *
 * Usage:
 *   node scripts/assert_unsafe_allowlist.mjs
 */

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const SCAN_ROOTS = [path.join(ROOT, "crates"), path.join(ROOT, "apps")];

/** Paths relative to the repo root, using forward slashes. */
const ALLOWLIST = new Set([
  "crates/renderpilot-nvapi/src/api.rs",
  "crates/renderpilot-nvapi/src/ffi.rs",
  "apps/desktop/src-tauri/src/elevation.rs",
  "apps/desktop/src-tauri/src/lib.rs", // std::env::set_var at single-threaded startup
]);

/**
 * Match `allow(unsafe_code)` and `expect(unsafe_code)` in outer attributes and
 * in `cfg_attr(..., allow|expect(unsafe_code))`.
 */
const ALLOW_RE =
  /(?:#!?\[|cfg_attr\s*\([^)]*,\s*)(?:allow|expect)\s*\(\s*[^)]*\bunsafe_code\b/;

function* walkRustFiles(dir) {
  if (!fs.existsSync(dir)) return;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "target" || entry.name === "node_modules") continue;
      yield* walkRustFiles(full);
    } else if (entry.isFile() && entry.name.endsWith(".rs")) {
      yield full;
    }
  }
}

function toPosixRel(fullPath) {
  return path.relative(ROOT, fullPath).split(path.sep).join("/");
}

function main() {
  const violations = [];
  const foundAllow = new Set();

  for (const root of SCAN_ROOTS) {
    for (const filePath of walkRustFiles(root)) {
      const rel = toPosixRel(filePath);
      const text = fs.readFileSync(filePath, "utf8");
      const normalized = text.replace(/\s+/g, " ");
      if (!ALLOW_RE.test(normalized) && !ALLOW_RE.test(text)) {
        continue;
      }
      foundAllow.add(rel);
      if (!ALLOWLIST.has(rel)) {
        violations.push(rel);
      }
    }
  }

  console.log("Approved unsafe_code allowlist:");
  for (const entry of [...ALLOWLIST].sort()) {
    const marker = foundAllow.has(entry) ? "ok" : "unused";
    console.log(`  [${marker}] ${entry}`);
  }

  if (violations.length > 0) {
    console.error("error: allow/expect(unsafe_code) outside allowlist:");
    for (const rel of violations) {
      console.error(`  ${rel}`);
    }
    process.exit(1);
  }

  const missing = [...ALLOWLIST].filter((entry) => !foundAllow.has(entry)).sort();
  if (missing.length > 0) {
    // Not fatal: an allow may be cfg-gated away on this platform.
    console.log("note: allowlist entries without a match in the tree:");
    for (const rel of missing) {
      console.log(`  ${rel}`);
    }
  }

  console.log("unsafe allowlist check passed");
  process.exit(0);
}

main();
