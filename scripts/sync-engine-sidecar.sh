#!/usr/bin/env bash
# Copy the fw-browser engine sidecar into a served pkg/ dir under
# CONTENT-HASHED names, and (re)write pkg/engine-manifest.json pointing at
# them.
#
# `just studio-fw-browser-sidecar` (wasm-bindgen) always emits UNHASHED
# names — fw_browser.js / fw_browser_bg.wasm — into its own sidecar dir;
# this script is what turns that into the hashed pair a served build ships.
# Hashed, not the wasm-bindgen originals: content-hashed names are what let
# lp-cloud-server's cache policy put these multi-MB files on the immutable
# tier (see lp-cloud/lp-cloud-server/src/page/cache_policy.rs) instead of
# the 5-minute default. The unhashed names are intentionally NOT also
# copied — the standalone fw-browser smoke page (lp-fw/fw-browser/www)
# keeps serving unhashed names from its own static tree, unrelated to this
# one, which is why `BrowserWorkerOptions::default()` keeps the unhashed
# constants as a fallback.
#
# Shared by every justfile recipe that copies the sidecar into a served
# pkg/ dir (`studio-web-copy-sidecars`, and `studio-dev`'s
# `sync_generated_assets` loop) so the hash-and-cleanup logic lives once.
# The `studio-dev` caller runs this every second against a live, never-wiped
# public dir — a sidecar rebuild mid-serve gets a NEW hash, so old hashed
# copies are removed first, or they would accumulate one stale pair per
# rebuild for the life of the server.
#
# Usage: scripts/sync-engine-sidecar.sh <sidecar_dir> <out_pkg_dir>
set -euo pipefail

sidecar_dir="$1"
out_pkg_dir="$2"

js_src="${sidecar_dir}/fw_browser.js"
wasm_src="${sidecar_dir}/fw_browser_bg.wasm"
if [[ ! -f "${js_src}" || ! -f "${wasm_src}" ]]; then
    echo "missing fw-browser sidecar artifacts in ${sidecar_dir}" >&2
    exit 1
fi

# shasum is macOS's tool; CI's Linux runners carry sha256sum instead.
sha256_hex() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    else
        sha256sum "$1" | cut -d' ' -f1
    fi
}

# A hash segment `looks_content_hashed`
# (lp-cloud/lp-cloud-server/src/page/cache_policy.rs) will actually treat as
# hashed: >=8 alphanumeric characters mixing at least one digit AND one
# letter. A hex digest slice mixes those virtually always, but nothing
# GUARANTEES it — 16 hex characters could land all-digit or all-letter — so
# this grows the slice from 16 hex chars in 8-char steps (up to the full
# 64-char digest) until both classes are present, instead of trusting the
# common case.
content_hash() {
    local full_hash mixed_len candidate has_digit has_alpha
    full_hash="$(sha256_hex "$1")"
    mixed_len=16
    while (( mixed_len <= ${#full_hash} )); do
        candidate="${full_hash:0:mixed_len}"
        has_digit=0
        has_alpha=0
        [[ "${candidate}" == *[0-9]* ]] && has_digit=1
        [[ "${candidate}" == *[a-f]* ]] && has_alpha=1
        if [[ "${has_digit}" == 1 && "${has_alpha}" == 1 ]]; then
            echo "${candidate}"
            return 0
        fi
        mixed_len=$((mixed_len + 8))
    done
    # sha256 hex is 64 chars; failing to mix even over the whole digest is
    # astronomically unlikely — fall back to it whole rather than loop
    # forever.
    echo "${full_hash}"
}

mkdir -p "${out_pkg_dir}"
js_hash="$(content_hash "${js_src}")"
wasm_hash="$(content_hash "${wasm_src}")"
js_name="fw_browser-${js_hash}.js"
wasm_name="fw_browser_bg-${wasm_hash}.wasm"

# Idempotence first: the `studio-dev` loop calls this every second, and on
# the 3599 seconds out of 3600 when nothing was rebuilt it must not touch
# the served dir at all — the sweep below opens a brief no-file window that
# a page fetch could land in, and re-copying an 8 MB wasm every second is
# pure churn besides.
if [[ -f "${out_pkg_dir}/${js_name}" && -f "${out_pkg_dir}/${wasm_name}" ]] \
    && grep -q "${wasm_name}" "${out_pkg_dir}/engine-manifest.json" 2>/dev/null \
    && grep -q "${js_name}" "${out_pkg_dir}/engine-manifest.json" 2>/dev/null; then
    exit 0
fi

# See the file header: a sidecar rebuild gets a NEW hash, so sweep the pair
# the previous build left behind before copying its replacement.
rm -f "${out_pkg_dir}"/fw_browser-*.js "${out_pkg_dir}"/fw_browser_bg-*.wasm
cp "${js_src}" "${out_pkg_dir}/${js_name}"
cp "${wasm_src}" "${out_pkg_dir}/${wasm_name}"

# Plain byte count: this is the raw .wasm on disk, before any gzip/brotli
# precompression (owned elsewhere) — the number the shell loader's progress
# bar wants is what a streaming fetch actually receives before decoding.
wasm_bytes="$(wc -c < "${wasm_src}" | tr -d '[:space:]')"
cat > "${out_pkg_dir}/engine-manifest.json" <<EOF
{"fw_browser_js":"/pkg/${js_name}","fw_browser_wasm":"/pkg/${wasm_name}","wasm_bytes":${wasm_bytes}}
EOF
