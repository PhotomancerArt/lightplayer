#!/usr/bin/env node
// Extract the embedded firmware manifest core from a build artifact (ELF,
// espflash merged image, or wasm module) by scanning for its delimiters.
//
// This is the CI-side twin of `lp-cli firmware show` — dependency-free so
// firmware CI jobs can drift-check without building lp-cli. The delimiters
// mirror `lp-core/lpc-model/src/manifest.rs` (MANIFEST_BLOB_BEGIN/END); the
// `delimiters_are_pinned` test in that file pins the exact byte forms — if it
// ever changes, update the BEGIN/END buffers here in the same commit.
//
// Usage:
//   node scripts/extract-fw-manifest.mjs <artifact>            # raw payload
//   node scripts/extract-fw-manifest.mjs <artifact> --stable   # provenance-
//     free pretty JSON (commit/dirty/profile stripped) for diffing against a
//     checked-in manifest-core.expected.json.

import { readFileSync } from "node:fs";

const BEGIN = Buffer.from("\x01LP-FW-MANIFEST-BEGIN-v1\x02", "latin1");
const END = Buffer.from("\x03LP-FW-MANIFEST-END-v1\x04", "latin1");

const [artifact, mode] = process.argv.slice(2);
if (!artifact) {
  console.error("usage: extract-fw-manifest.mjs <artifact> [--stable]");
  process.exit(2);
}

const bytes = readFileSync(artifact);
const start = bytes.indexOf(BEGIN);
if (start === -1) {
  console.error(`no firmware manifest core found in ${artifact}`);
  process.exit(1);
}
const payloadStart = start + BEGIN.length;
const end = bytes.indexOf(END, payloadStart);
if (end === -1) {
  console.error(`manifest core in ${artifact} is truncated (no END delimiter)`);
  process.exit(1);
}
const payload = bytes.subarray(payloadStart, end).toString("utf8");

if (mode === "--stable") {
  const core = JSON.parse(payload);
  delete core.commit;
  delete core.dirty;
  delete core.profile;
  console.log(JSON.stringify(core, null, 2));
} else {
  console.log(payload);
}
