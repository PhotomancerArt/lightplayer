#!/usr/bin/env bash
# Build the ESP32-C6 second-stage bootloader from ESP-IDF source, in Docker,
# reproducibly. Nothing is installed on the host: the toolchain, Python and
# ESP-IDF itself all come from one espressif/idf image pinned by digest.
#
#   scripts/bootloader/build-c6-bootloader.sh [--variant quiet|stock] [--out <dir>]
#                                             [--platform linux/arm64|linux/amd64]
#                                             [--check]
#
#   --variant quiet (default)  the bootloader LightPlayer ships: the stock one
#                              with its log level set to NONE
#   --variant stock            espflash 3.3.0's bundled bootloader, rebuilt —
#                              the proof that this recipe IS that bootloader
#                              (compared byte for byte, see --check)
#   --out                      default target/bootloader/<variant>/
#   --platform                 the image's architecture (default: Docker's);
#                              both give the same bytes, which is checked by
#                              running it once under each
#   --check                    compare the result with the committed binary
#                              (quiet) or espflash 3.3.0's bundled one (stock)
#                              and fail on any difference
#
# What is pinned, and why each:
#
# * IMAGE: espressif/idf:v5.1, by the digest of its multi-arch index (so the
#   same pin serves arm64 and amd64). It carries the RISC-V GCC
#   (esp-12.2.0_20230208), CMake 3.24, Ninja, the Python env and a FULL clone
#   of ESP-IDF, history included — the pinned commit below is already in it,
#   so the build never touches the network.
# * IDF_COMMIT: the commit espflash 3.3.0's bundled esp32c6-bootloader.bin
#   was built from (`v5.1-beta1-378-gea5e0ff298`, from its version string; on
#   the release/v5.1 branch, an ancestor of the v5.1 tag the image carries).
#   The image's tools.json at that commit names the same toolchain.
# * IDF_VER: the version string IDF compiles in. espflash's build said
#   `v5.1-beta1-378-gea5e0ff298-dirty` (truncated to 31 bytes in the banner);
#   pinned rather than `git describe`d, which depends on the clone's tags.
# * SOURCE_DATE_EPOCH: GCC's __DATE__/__TIME__. 1686124928 is
#   2023-06-07 08:02:08 UTC, the stock banner's `compile time`. The quiet
#   variant logs nothing, so the date leaves its bytes untouched either way.
#
# Provenance: lp-fw/bootloaders/README.md. License: ESP-IDF is Apache-2.0
# (licenses/ESP-IDF-Apache-2.0.txt).
set -euo pipefail

IMAGE="espressif/idf@sha256:16beaf5b7f54d343f7c38d2738c7c03829fef78f7555e23eac4d20dc8515eb45"
IDF_COMMIT="ea5e0ff298e6257b31d8e0c81435e6d3937f04c7"
IDF_VER="v5.1-beta1-378-gea5e0ff298-dirty"
SOURCE_DATE_EPOCH_PIN="1686124928"

# The committed quiet binary, and the stock one it is checked against.
QUIET_BIN="lp-fw/bootloaders/esp32c6-bootloader-idf-v5.1-quiet.bin"
STOCK_SHA256="c569fa7d2363da15f6b60ea288aa2396ca3c33b6eed84616bb506feaba7a0528"

variant=quiet
out=""
platform=""
check=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --variant) variant="${2:?--variant needs quiet or stock}"; shift 2 ;;
        --out) out="${2:?--out needs a directory}"; shift 2 ;;
        --platform) platform="${2:?--platform needs a value}"; shift 2 ;;
        --check) check=1; shift ;;
        -h|--help) sed -n '2,25p' "$0"; exit 0 ;;
        *) echo "build-c6-bootloader: unknown argument $1" >&2; exit 2 ;;
    esac
done
case "$variant" in quiet|stock) ;; *) echo "build-c6-bootloader: --variant is quiet or stock" >&2; exit 2 ;; esac

repo="$(cd "$(dirname "$0")/../.." && pwd)"
out="${out:-$repo/target/bootloader/$variant${platform:+-${platform##*/}}}"
docker="${DOCKER:-$(command -v docker || echo /usr/local/bin/docker)}"
if [[ ! -x "$docker" ]]; then
    echo "build-c6-bootloader: Docker is not installed (looked for \`docker\` on PATH and /usr/local/bin/docker)." >&2
    exit 127
fi

rm -rf "$out"
mkdir -p "$out"
# Docker takes a relative path for a named volume, not a directory.
out="$(cd "$out" && pwd)"
echo "build-c6-bootloader: $variant, ESP-IDF ${IDF_COMMIT:0:10} in $IMAGE${platform:+ ($platform)}" >&2
"$docker" run --rm \
    ${platform:+--platform "$platform"} \
    --network none \
    -v "$repo/scripts/bootloader:/src:ro" \
    -v "$out:/out" \
    -e HOST_OWNER="$(id -u):$(id -g)" \
    "$IMAGE" \
    /src/in-container.sh "$IDF_COMMIT" "$variant" "$SOURCE_DATE_EPOCH_PIN" "$IDF_VER" \
    >"$out/docker.log" 2>&1 || {
    tail -40 "$out/docker.log" >&2
    echo "build-c6-bootloader: the build failed; whole log in $out/docker.log" >&2
    exit 1
}

sha="$(shasum -a 256 "$out/bootloader.bin" | awk '{print $1}')"
echo "$sha  bootloader.bin" > "$out/bootloader.bin.sha256"
echo "build-c6-bootloader: $out/bootloader.bin ($(wc -c < "$out/bootloader.bin" | tr -d ' ') B) sha256 $sha"

if [[ "$check" == 1 ]]; then
    if [[ "$variant" == quiet ]]; then
        want="$(shasum -a 256 "$repo/$QUIET_BIN" | awk '{print $1}')"
        what="the committed $QUIET_BIN"
    else
        want="$STOCK_SHA256"
        what="espflash 3.3.0's bundled esp32c6-bootloader.bin"
    fi
    if [[ "$sha" != "$want" ]]; then
        echo "build-c6-bootloader: MISMATCH — built $sha, $what is $want" >&2
        exit 1
    fi
    echo "build-c6-bootloader: byte-identical to $what"
fi
