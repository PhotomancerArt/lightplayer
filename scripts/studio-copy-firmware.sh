#!/usr/bin/env bash
# Copy one served build's packaged firmware into a Studio bundle:
#
#   scripts/studio-copy-firmware.sh <build_id> <assets_firmware_dir> <dest_firmware_dir> <parts_root>
#
#   <assets_firmware_dir>/<build_id>/   manifest.json + the merged image (`lp-cli firmware package`)
#   <parts_root>/<build_id>/            a split package's parts and OTA files
#   <dest_firmware_dir>/<build_id>/     what the bundle serves
#
# Always: `manifest.json` and the `*.bin` beside it (the flasher's merged
# image). For a SPLIT package (its manifest has a `split` block) also the
# package's update files, beside the manifest in `<dest>/<build_id>/` (OTA
# M7 P8, DS10): `ota-manifest.json`, `core.z`, `engine.z` — never `core.bin` /
# `engine.bin`, which Studio slices out of the merged image by the `split`
# offsets. They must be THIS package's: `ota-manifest.json`'s `package`
# entry must hash the copied `manifest.json`, or the build fails with the
# recipe to run (a stale parts directory from another package is the case
# this catches). A single-image package (the fast local build) gets none
# of the three, and stale ones in the destination are removed, so the bundle
# never offers an update its firmware cannot take.
#
# Two segments under `firmware/`, never three: on lightplayer.app every
# `/firmware/<target>/<release>/<file>` path belongs to lp-cloud-server's
# firmware lookup, which answers it before the static bundle is consulted
# (docs/defects/2026-10-06-the-bundles-ota-files-are-shadowed-by-the-firmware-lookup.md).
#
# Used by `studio-web-copy-sidecars` and `studio-dev`'s asset sync loop.
set -euo pipefail

build_id="$1"
assets="$2"
dest="$3"
parts_root="$4"

src="${assets}/${build_id}"
out="${dest}/${build_id}"
parts="${parts_root}/${build_id}"

mkdir -p "${out}"
cp "${src}/manifest.json" "${out}/manifest.json"
cp "${src}"/*.bin "${out}/"

split="$(node -e '
const m = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
process.stdout.write(m.split ? "yes" : "no");
' "${src}/manifest.json")"

# The update files' old home (`ota/`, three segments deep) never comes back.
rm -rf "${out}/ota"

if [[ "${split}" != "yes" ]]; then
    rm -f "${out}/ota-manifest.json" "${out}/core.z" "${out}/engine.z"
    exit 0
fi

for file in ota-manifest.json core.z engine.z; do
    if [[ ! -f "${parts}/${file}" ]]; then
        echo "studio-copy-firmware: ${build_id} is a split package but ${parts}/${file} is missing" >&2
        echo "  run: just studio-firmware-package-served split   (or: cargo run -p lp-cli -- firmware package ${build_id})" >&2
        exit 1
    fi
done

# The update files must describe the package being copied.
node -e '
const fs = require("fs");
const crypto = require("crypto");
const [otaPath, manifestPath] = process.argv.slice(1);
const ota = JSON.parse(fs.readFileSync(otaPath, "utf8"));
const bytes = fs.readFileSync(manifestPath);
const sha = crypto.createHash("sha256").update(bytes).digest("hex");
if (!ota.package || ota.package.sha256 !== sha || ota.package.length !== bytes.length) {
  console.error(`studio-copy-firmware: ${otaPath} describes another package than ${manifestPath}`);
  console.error("  run: just studio-firmware-package-served split");
  process.exit(1);
}
' "${parts}/ota-manifest.json" "${src}/manifest.json"

cp "${parts}/ota-manifest.json" "${parts}/core.z" "${parts}/engine.z" "${out}/"
