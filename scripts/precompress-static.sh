#!/usr/bin/env bash
# Precompress a built Studio artifact for negotiated serving.
#
# Lays a `<file>.br` twin beside every large text-like asset so the server
# (lp-cloud-server page plane, `StaticSite::file_negotiated`) can answer
# brotli-accepting requests from bytes compressed ONCE at build time —
# Fly's proxy does not compress, and runtime q11 brotli of an 11 MB wasm
# per cold cache would be absurd. Measured on the 2026-08-18 artifact:
# app wasm 11.5 MB -> 2.9 MB, engine wasm 8.0 MB -> 1.2 MB.
#
# Idempotent: re-running refreshes twins whose original changed and removes
# orphans. Serving ignores orphaned twins either way; the cleanup is for
# artifact hygiene, not correctness.
#
# Used by infra/Dockerfile (webcompress stage). Run locally to test
# negotiation against a `just studio-web-deploy-dir` output:
#
#   scripts/precompress-static.sh target/pages/studio
set -euo pipefail

dir="${1:?usage: precompress-static.sh <artifact-dir>}"
[[ -d "$dir" ]] || { echo "not a directory: $dir" >&2; exit 1; }
command -v brotli >/dev/null || { echo "brotli not installed" >&2; exit 1; }

# Compressible types only. Skips already-entropy-dense formats (png, woff2)
# and skips the documents: index.html is served from memory, per-request
# mutated (OG injection), and no-cache — a twin would never be read.
min_bytes=1024
compressed=0
skipped=0

while IFS= read -r -d '' file; do
    size=$(wc -c < "$file")
    if (( size < min_bytes )); then
        skipped=$((skipped + 1))
        continue
    fi
    twin="${file}.br"
    if [[ -f "$twin" && "$twin" -nt "$file" ]]; then
        skipped=$((skipped + 1))
        continue
    fi
    brotli --force --quality=11 --output="$twin" -- "$file"
    compressed=$((compressed + 1))
done < <(find "$dir" -type f \
    \( -name '*.wasm' -o -name '*.js' -o -name '*.mjs' -o -name '*.css' \
       -o -name '*.json' -o -name '*.map' -o -name '*.svg' \) \
    ! -name '*.br' -print0)

# Orphaned twins: an original deleted between runs leaves a twin nothing
# will serve.
while IFS= read -r -d '' twin; do
    [[ -f "${twin%.br}" ]] || { rm -- "$twin"; echo "removed orphan: $twin"; }
done < <(find "$dir" -type f -name '*.br' -print0)

echo "precompress: $compressed compressed, $skipped skipped in $dir"
