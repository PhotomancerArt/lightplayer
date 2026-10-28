#!/usr/bin/env bash
# A PUBLISHED release, laid out the way `scripts/ota/build-image.sh` lays out
# an image it builds, so every emulator OTA test and walk can board it as X:
# the crossing walks of a format bump (a board on the last release before
# the bump, updated to this tree) need a fielded release's own bytes, never
# a rebuild of it from this tree. Fetched from the firmware store
# (`<store>/firmware/<target>/<release>/<file>`, the URLs Studio reads,
# `docs/adr/2026-10-06-firmware-distribution.md`), each file checked
# against the release's own `package.json`/`ota-manifest.json` hashes.
# <out> gets:
#
#   merged.bin   the release's merged image (behind our own bootloader),
#                padded with 0xff to the whole 4 MiB chip: a board as the
#                release's USB flash leaves it, its lpfs erased
#   split.json   the split's account, from the package's `split` block
#                (buildId, loader/core/engine offsets as numbers)
#   package/     manifest.json (the release's package.json) + the merged image
#   ota/         ota-manifest.json, core.bin, engine.bin, core.z, engine.z
#   release.json what was fetched: release, target, commit, wireProto, store
#
#   scripts/ota/fetch-release-image.sh <out> <release> [target]
#
# `LP_FIRMWARE_STORE` overrides the store origin (default
# https://lightplayer.app). No merged-idf.bin: a release ships one door.
set -euo pipefail

out="$1"
release="$2"
target="${3:-esp32c6-4mb}"
store="${LP_FIRMWARE_STORE:-https://lightplayer.app}"
case "$target" in
    esp32c6-4mb) chip=esp32c6; flash_bytes=$((4 * 1024 * 1024)) ;;
    esp32s3-8mb) chip=esp32s3; flash_bytes=$((8 * 1024 * 1024)) ;;
    *) echo "fetch-release-image: target $target: expected esp32c6-4mb or esp32s3-8mb" >&2; exit 2 ;;
esac
merged_name="fw-${chip}-merged.bin"

rm -rf "$out"
mkdir -p "$out/ota" "$out/package"
fetch() {
    curl -fsSL --retry 3 -o "$2" "$store/firmware/$target/$release/$1" || {
        echo "fetch-release-image: $store/firmware/$target/$release/$1 did not fetch" >&2
        exit 1
    }
}
check() {
    local got
    got="$(shasum -a 256 "$1" | cut -d' ' -f1)"
    [[ "$got" == "$2" ]] || {
        echo "fetch-release-image: $1: sha256 $got, the release says $2" >&2
        exit 1
    }
}

fetch package.json "$out/package/manifest.json"
fetch ota-manifest.json "$out/ota/ota-manifest.json"
manifest="$out/ota/ota-manifest.json"
jq -e --arg r "$release" --arg t "$target" '.version == $r and .target == $t' "$manifest" >/dev/null || {
    echo "fetch-release-image: the store's ota-manifest.json is not $release for $target" >&2
    exit 1
}
for part in core engine; do
    fetch "$part.bin" "$out/ota/$part.bin"
    check "$out/ota/$part.bin" "$(jq -r ".$part.sha256" "$manifest")"
    z="$(jq -r ".encodings[]? | select(.id == 1) | .$part.file // empty" "$manifest")"
    if [[ -n "$z" ]]; then
        fetch "$z" "$out/ota/$z"
        check "$out/ota/$z" "$(jq -r ".encodings[] | select(.id == 1) | .$part.sha256" "$manifest")"
    fi
done
fetch "$merged_name" "$out/package/$merged_name"
check "$out/package/$merged_name" "$(jq -r --arg n "$merged_name" '.images[] | select(.path == $n) | .sha256' "$out/package/manifest.json")"

# The whole chip: the merged image, then erased flash to the chip's end.
cp "$out/package/$merged_name" "$out/merged.bin"
size=$(wc -c < "$out/merged.bin" | tr -d ' ')
if (( size < flash_bytes )); then
    head -c $((flash_bytes - size)) /dev/zero | LC_ALL=C tr '\0' '\377' >> "$out/merged.bin"
fi

# The package's offsets are hex strings; split.json's are numbers.
python3 -I - "$out/package/manifest.json" "$out/split.json" "$chip" <<'PY'
import json, sys
package = json.load(open(sys.argv[1]))
split = package["split"]
def part(p):
    q = dict(p)
    q["offset"] = int(q["offset"], 16) if isinstance(q["offset"], str) else q["offset"]
    return q
out = {
    "buildId": split["buildId"],
    "layout": split.get("layout"),
    "page": split.get("page"),
    "chip": sys.argv[3],
    "loader": part(split["loader"]),
    "core": part(split["core"]),
    "engine": part(split["engine"]),
}
json.dump(out, open(sys.argv[2], "w"), indent=2)
PY

jq --arg store "$store" '{release: .version, target, commit, wireProto, projectFormat, store: $store}' "$manifest" > "$out/release.json"
echo "fetch-release-image: $out ($(jq -r '.buildId' "$out/split.json"), commit $(jq -r .commit "$manifest"), wire $(jq -r .wireProto "$manifest"))"
