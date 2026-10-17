#!/usr/bin/env bash
# Runs INSIDE the pinned espressif/idf image; build-c6-bootloader.sh is the
# only caller. Builds the ESP32-C6 second-stage bootloader at the pinned
# ESP-IDF commit and copies it, its sdkconfig and its ELF to /out.
#
#   in-container.sh <idf-commit> <variant> <source-date-epoch> <idf-ver>
#
# /src is this directory (read only), /out the output directory. Nothing here
# reaches the network: the image carries ESP-IDF's whole history, so the
# commit is checked out of the image's own clone (the container is thrown
# away afterwards, so the checkout never outlives the build).
set -euo pipefail

idf_commit="$1"
variant="$2"
epoch="$3"
idf_ver="$4"

cd "$IDF_PATH"
git -c advice.detachedHead=false checkout -q --detach "$idf_commit"
# The only submodule that differs between the image's v5.1 and the pinned
# commit is components/esp_wifi/lib, which the bootloader never builds; the
# check below fails the build if that ever stops being true.
git submodule status --recursive | awk '$1 ~ /^\+/ {print $2}' > /tmp/moved-submodules
if grep -v '^components/esp_wifi/lib$' /tmp/moved-submodules; then
    echo "in-container: submodules above moved between the image and $idf_commit" >&2
    exit 1
fi
# IDF reads its version from version.txt before `git describe`; pinning it
# keeps the string independent of which tags the image's clone carries.
echo "$idf_ver" > version.txt

# shellcheck disable=SC1091
. "$IDF_PATH/export.sh" >/dev/null

rm -rf /build && mkdir -p /build
cp -r /src/c6-project /build/proj
cd /build/proj
defaults="sdkconfig.defaults;sdkconfig.$variant"
[[ -f "/src/c6-project/sdkconfig.$variant" ]] || { echo "in-container: no sdkconfig.$variant" >&2; exit 2; }

export SOURCE_DATE_EPOCH="$epoch"
idf.py -DIDF_TARGET=esp32c6 -DSDKCONFIG_DEFAULTS="$defaults" bootloader >/build/idf.log 2>&1 || {
    tail -60 /build/idf.log >&2
    exit 1
}

cp build/bootloader/bootloader.bin /out/bootloader.bin
cp build/bootloader/bootloader.elf /out/bootloader.elf
cp build/bootloader/bootloader.map /out/bootloader.map
cp sdkconfig /out/sdkconfig
# The non-default lines: what this build changed from ESP-IDF's own defaults
# for the C6 at this commit (`save-defconfig` rewrites the project's
# sdkconfig.defaults with exactly those, after the build is done with it).
idf.py -DIDF_TARGET=esp32c6 -DSDKCONFIG_DEFAULTS="$defaults" save-defconfig >>/build/idf.log 2>&1
cp sdkconfig.defaults /out/sdkconfig.non-default
esptool.py --chip esp32c6 image_info --version 2 /out/bootloader.bin > /out/image-info.txt 2>&1 || true
# The container runs as root (the build moves the image's ESP-IDF to the
# pinned commit); hand the outputs back to whoever ran the script.
if [[ -n "${HOST_OWNER:-}" ]]; then
    chown -R "$HOST_OWNER" /out
fi
