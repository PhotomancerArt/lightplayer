#!/usr/bin/env node
// The JSON and hash checks fetch-release-firmware.sh makes on a release's
// files, in one place (node, which every deploy runner has; no lp-cli build).
//
//   release-files.mjs package <package.json> <target> <version>
//       check the package is <target>'s at <version>; print one line per
//       image ("image\t<file>\t<sizeBytes>\t<sha256>") then "split\tyes|no"
//   release-files.mjs image <file> <sizeBytes> <sha256>
//       check one image against what its package manifest says
//   release-files.mjs ota <dir> <target>
//       check <dir>/<target>.ota-manifest.json names <dir>/<target>.package.json
//       (its `package` entry) and <dir>/<target>.core.z / .engine.z (length
//       and sha256 of the files it lists) — the rule
//       scripts/studio-copy-firmware.sh and Studio's BundledOwnBuild enforce
//   release-files.mjs sha256 <file>
//       print the file's SHA-256
//
// Any mismatch exits 1 with one line naming it.

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function fail(message) {
  console.error(message);
  process.exit(1);
}

const [command, ...args] = process.argv.slice(2);
switch (command) {
  case "package": {
    const [path, target, version] = args;
    const manifest = JSON.parse(readFileSync(path, "utf8"));
    if (manifest.firmwareId !== target) {
      fail(`the package is ${manifest.firmwareId}, not ${target}`);
    }
    const carried = manifest.core && manifest.core.version;
    if (carried !== version) {
      fail(`${target} carries version ${carried}, not ${version}`);
    }
    for (const image of manifest.images) {
      console.log(["image", image.path, image.sizeBytes, image.sha256].join("\t"));
    }
    console.log(["split", manifest.split ? "yes" : "no"].join("\t"));
    break;
  }
  case "image": {
    const [path, size, want] = args;
    const bytes = readFileSync(path);
    const got = sha256(bytes);
    if (bytes.length !== Number(size) || got !== want) {
      fail(
        `${path}: ${bytes.length} bytes, sha256 ${got}; ` +
          `the package manifest says ${size} bytes, sha256 ${want}`,
      );
    }
    break;
  }
  case "ota": {
    const [dir, target] = args;
    const read = (file) => readFileSync(`${dir}/${target}.${file}`);
    const ota = JSON.parse(read("ota-manifest.json"));
    // Every { file, length, sha256 } the manifest lists, by file name.
    const files = {};
    (function walk(value) {
      if (value && typeof value === "object") {
        if (typeof value.file === "string" && typeof value.sha256 === "string") {
          files[value.file] = value;
        }
        for (const key of Object.keys(value)) walk(value[key]);
      }
    })(ota);
    const pkg = read("package.json");
    if (!ota.package || ota.package.sha256 !== sha256(pkg) || ota.package.length !== pkg.length) {
      fail(`${target}.ota-manifest.json describes another package than ${target}.package.json`);
    }
    for (const file of ["core.z", "engine.z"]) {
      const want = files[file];
      const bytes = read(file);
      if (!want || want.sha256 !== sha256(bytes) || want.length !== bytes.length) {
        fail(`${target}.${file} is not the file ${target}.ota-manifest.json names`);
      }
    }
    break;
  }
  case "sha256": {
    process.stdout.write(sha256(readFileSync(args[0])));
    break;
  }
  default:
    fail(`usage: release-files.mjs package|image|ota|sha256 …`);
}
