#!/usr/bin/env bash
# Copy the fw-browser engine sidecar — and the ESP32-C6 emulator module
# beside it, when there is one — into a served pkg/ dir under CONTENT-HASHED
# names, and (re)write pkg/engine-manifest.json pointing at them.
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
# The emulator module (`lp_emu_esp32c6.wasm`, laid down by `just
# studio-emu-sidecar`) rides the same hashing for the same reason — it is
# multi-MB and immutable. Its ABSENCE is not an error: the story build and
# the standalone smoke pages do not need it, and a tree that has never run
# `just emu-c6-wasm` still serves Studio. Missing means no key in the
# manifest, which is exactly what the emu row's "only when a module exists"
# rule (D21) reads. Since M7 P7 the module travels with `jit-host.js` — it
# imports `emu_host` and does not link without it — so the two keys appear and
# disappear together.
#
# NO HASHING WHEN NOTHING MOVED. The once-a-second caller is the reason the
# script keeps a stamp (`.engine-sidecar.stamp`, beside the manifest): one
# line per source with its size, its mtime and the hashed name it produced.
# When every stat still matches and every named copy is still served, the
# script exits without opening a source at all. Before the stamp existed the
# idempotence check sat BELOW the hashing, so a dev server read and
# SHA-256'd ~24 MB every second for the life of the session and threw the
# answer away on 3599 seconds out of 3600 (measured 2026-09-11:
# fw_browser_bg.wasm 21.5 MB + lp_emu_esp32c6.wasm 2.3 MB). The CONTENT hash
# is still what names the copy — the stamp is a memo of a hash run, never a
# substitute for one — because `looks_content_hashed` reads that segment.
#
# Usage: scripts/sync-engine-sidecar.sh <sidecar_dir> <out_pkg_dir>
set -euo pipefail

sidecar_dir="$1"
out_pkg_dir="$2"

js_src="${sidecar_dir}/fw_browser.js"
wasm_src="${sidecar_dir}/fw_browser_bg.wasm"
emu_src="${sidecar_dir}/lp_emu_esp32c6.wasm"
if [[ ! -f "${js_src}" || ! -f "${wasm_src}" ]]; then
    echo "missing fw-browser sidecar artifacts in ${sidecar_dir}" >&2
    exit 1
fi

# ---- the emulator's JS host -------------------------------------------------
#
# The emulator module imports the namespace `emu_host` — since M7 P7 the wasm
# build installs a translated core by default — and cannot be instantiated
# without it, so the module and `jit-host.js` travel as a PAIR: a served
# `emu_esp32c6_wasm` with no `emu_jit_host_js` beside it is a board that cannot
# boot. Hashed for the same reason as the rest, and read straight out of the
# repo rather than out of the sidecar dir, because it is source and nothing
# builds it.
#
# Never a hand copy under `public/lpa-link/`: it is M7's file, it is MOVING
# (P9 relocates it to `lp-emu/lp-emu-jit/js/` and leaves a re-export shim at
# the old path), and a shim copied here would import a path that does not
# exist next to it. `jit-host-source.sh` is the one resolver and it refuses a
# shim; this script only asks.
repo="$(cd "$(dirname "$0")/.." && pwd)"
host_src=""
if [[ -f "${emu_src}" ]]; then
    host_src="$("${repo}/scripts/emu/jit-host-source.sh")"
fi

manifest="${out_pkg_dir}/engine-manifest.json"
stamp="${out_pkg_dir}/.engine-sidecar.stamp"

# `<size> <mtime>` for a file — the two facts a rebuild cannot leave alone.
# macOS's BSD stat and coreutils' spell it differently; try BSD first because
# that is the desk, and fall through to coreutils for CI's Linux runners.
fingerprint() {
    if stat -f '%z %m' "$1" 2>/dev/null; then
        return 0
    fi
    stat -c '%s %Y' "$1"
}

# ---- the fast path: three stats, no hashing --------------------------------
#
# The stamp only ever memoises what a real hash run saw, and every claim it
# makes is re-checked against the served dir — the hashed copy must be there
# and the manifest must name it — so a wiped `public/` (dx rebuilds it), a
# hand-deleted copy, or a module that has appeared since (`just emu-c6-wasm`
# mid-serve) all fall through to the slow path, exactly as before.
js_fp="$(fingerprint "${js_src}")"
wasm_fp="$(fingerprint "${wasm_src}")"
emu_fp="absent"
[[ -f "${emu_src}" ]] && emu_fp="$(fingerprint "${emu_src}")"
# A fourth line, on the same rule as the third: `absent` whenever there is no
# module to pair it with, so an emulator that has come or gone takes its host
# with it.
host_fp="absent"
[[ -n "${host_src}" ]] && host_fp="$(fingerprint "${host_src}")"

if [[ -f "${stamp}" && -f "${manifest}" ]]; then
    current=1
    lines=0
    while IFS= read -r line; do
        lines=$((lines + 1))
        case "${lines}" in
            1) want="${js_fp}" ;;
            2) want="${wasm_fp}" ;;
            3) want="${emu_fp}" ;;
            4) want="${host_fp}" ;;
            *) current=0; break ;;
        esac
        if [[ "${want}" == "absent" ]]; then
            # No module now: the stamp must agree, or a module that was
            # served has gone and the manifest's key has to go with it.
            [[ "${line}" == "absent" ]] || current=0
            continue
        fi
        # `<size> <mtime> <name>`: the stat must match, and the name must
        # still be served and still be in the manifest.
        if [[ "${line}" != "${want} "* ]]; then
            current=0
            continue
        fi
        name="${line#"${want}" }"
        if [[ -z "${name}" || ! -f "${out_pkg_dir}/${name}" ]] \
            || ! grep -q -- "${name}" "${manifest}" 2>/dev/null; then
            current=0
        fi
    done < "${stamp}"
    if [[ "${current}" == 1 && "${lines}" == 4 ]]; then
        exit 0
    fi
fi

# ---- the slow path: hash, sweep, copy, write -------------------------------
#
# The fingerprints above were taken BEFORE the hashes below, on purpose: a
# source rewritten between the stat and the hash then mismatches its stamp
# on the next tick and is hashed again, where the other order could memo a
# stale hash under a fresh fingerprint.

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

emu_name=""
host_name=""
if [[ "${emu_fp}" != "absent" ]]; then
    emu_name="lp_emu_esp32c6-$(content_hash "${emu_src}").wasm"
    host_name="jit-host-$(content_hash "${host_src}").js"
fi

# Written last, after everything it vouches for is on disk — so a run killed
# half way leaves no stamp, and the next tick does the whole job again.
write_stamp() {
    local emu_line="absent"
    local host_line="absent"
    [[ -n "${emu_name}" ]] && emu_line="${emu_fp} ${emu_name}"
    [[ -n "${host_name}" ]] && host_line="${host_fp} ${host_name}"
    printf '%s %s\n%s %s\n%s\n%s\n' \
        "${js_fp}" "${js_name}" "${wasm_fp}" "${wasm_name}" \
        "${emu_line}" "${host_line}" \
        > "${stamp}"
}

# The idempotence check from before the stamp existed, kept for the one case
# a stat cannot see: a source touched (new mtime) but byte-identical, which
# hashes to the names already served. Then there is still nothing to copy
# — the sweep below opens a brief no-file window that a page fetch could land
# in — only a stamp to refresh so the next tick is a fast one again.
emu_is_current=1
if [[ -n "${emu_name}" ]]; then
    # The PAIR, both times: a module whose host is missing from the served dir
    # or from the manifest is a module nothing can instantiate.
    [[ -f "${out_pkg_dir}/${emu_name}" && -f "${out_pkg_dir}/${host_name}" ]] \
        && grep -q "${emu_name}" "${manifest}" 2>/dev/null \
        && grep -q "${host_name}" "${manifest}" 2>/dev/null \
        || emu_is_current=0
elif grep -q "emu_esp32c6_wasm\|emu_jit_host_js" "${manifest}" 2>/dev/null; then
    # The module has GONE since the manifest was written: "missing means no
    # key" has to hold on the way out as well as on the way in.
    emu_is_current=0
fi
if [[ -f "${out_pkg_dir}/${js_name}" && -f "${out_pkg_dir}/${wasm_name}" ]] \
    && grep -q "${wasm_name}" "${manifest}" 2>/dev/null \
    && grep -q "${js_name}" "${manifest}" 2>/dev/null \
    && [[ "${emu_is_current}" == 1 ]]; then
    write_stamp
    exit 0
fi

# See the file header: a sidecar rebuild gets a NEW hash, so sweep the pair
# the previous build left behind before copying its replacement.
rm -f "${out_pkg_dir}"/fw_browser-*.js "${out_pkg_dir}"/fw_browser_bg-*.wasm
cp "${js_src}" "${out_pkg_dir}/${js_name}"
cp "${wasm_src}" "${out_pkg_dir}/${wasm_name}"

# Swept on the same rule and for the same reason as the pair above: a
# rebuilt module gets a new hash, and `studio-dev` runs this every second
# for the life of the server. Swept whether or not a module is there now — a
# copy of one that has gone is exactly the stale file this sweep exists for.
rm -f "${out_pkg_dir}"/lp_emu_esp32c6-*.wasm "${out_pkg_dir}"/jit-host-*.js
emu_key=""
if [[ -n "${emu_name}" ]]; then
    cp "${emu_src}" "${out_pkg_dir}/${emu_name}"
    cp "${host_src}" "${out_pkg_dir}/${host_name}"
    emu_key=",\"emu_esp32c6_wasm\":\"/pkg/${emu_name}\",\"emu_wasm_bytes\":$(wc -c < "${emu_src}" | tr -d '[:space:]')"
    emu_key="${emu_key},\"emu_jit_host_js\":\"/pkg/${host_name}\""
fi

# Plain byte count: this is the raw .wasm on disk, before any gzip/brotli
# precompression (owned elsewhere) — the number the shell loader's progress
# bar wants is what a streaming fetch actually receives before decoding.
wasm_bytes="$(wc -c < "${wasm_src}" | tr -d '[:space:]')"
cat > "${manifest}" <<EOF
{"fw_browser_js":"/pkg/${js_name}","fw_browser_wasm":"/pkg/${wasm_name}","wasm_bytes":${wasm_bytes}${emu_key}}
EOF
write_stamp
