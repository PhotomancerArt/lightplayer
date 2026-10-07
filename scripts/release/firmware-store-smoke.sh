#!/usr/bin/env bash
# The firmware store's whole lookup, locally and hermetically: real staged
# release assets → a static GitHub-shaped upstream → lp-cloud-server's
# `/firmware/` route and its release index → curl. Plan D20: the pre-merge
# proof of the store; the live proof is the merged release's (yona-ship).
#
#   scripts/release/firmware-store-smoke.sh [<staging_dir>]
#
#   <staging_dir>   a release staging directory (release-firmware.sh's
#                   output) at a RELEASE version (`YYYY.MM.DD-N`). Without
#                   it, the C6 is packaged and staged at the synthetic
#                   release version 2099.01.01-1 (`release-firmware.sh
#                   --dry-run`): the route refuses dev versions by design,
#                   and the version must be right at package time (it is in
#                   the image and in ota-manifest.json). That re-packages the
#                   local C6 firmware at 2099.01.01-1; the next `studio-dev`
#                   packages its own again.
#
# Lays the staging directory out as GitHub serves a release
# (`download/v<version>/<asset>`, `latest/download/<asset>`) under a temp
# root, beside a GitHub-shaped REST releases list (`releases.json`: the one
# release, every staged asset `uploaded`), serves it with python3's
# http.server, starts lp-cloud-server with LP_CLOUD_FIRMWARE_UPSTREAM and
# LP_CLOUD_FIRMWARE_RELEASES_LIST pointed at it (both on scripts/dev-port.sh
# ports, never pinned), and asserts:
#
#   - the manifest by version: byte-equal, ACAO *, an ETag, immutable;
#   - `latest`: 302 to the version path, max-age=60;
#   - a tampered core.z upstream, asked before it is cached: 502;
#   - every file the manifest names: byte-equal, ETag = its sha256;
#   - a second engine.bin: no upstream request;
#   - a dev version and a reserved word: 404, no upstream request;
#   - the build id (`<version>+<commit[..12]>`): the version's bytes;
#   - the release index `/api/v1/firmware/<target>/releases`: 200, format 1,
#     the staged version and commit listed, ACAO *, max-age=60, an ETag;
#   - `/firmware/<target>/releases` is not the index (no ACAO, no index body).
#
# Needs: python3, curl, node, cargo. Everything is torn down on exit.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${here}/../.." && pwd)"
cd "${repo_root}"

target="esp32c6-4mb"
smoke_version="2099.01.01-1"
staging="${1:-}"

if [[ -z "${staging}" ]]; then
    staging="target/release-assets/${smoke_version}"
    echo "== staging the C6 at ${smoke_version} (release-firmware.sh --dry-run)"
    APP_VERSION="${smoke_version}" "${here}/release-firmware.sh" "${smoke_version}" \
        --dry-run --targets "${target}" --staging "${staging}" >/dev/null
fi
staging="$(cd "${staging}" && pwd)"

files_of() {
    node "${here}/release-files.mjs" files "${staging}/${target}.ota-manifest.json"
}
version="$(files_of | awk -F '\t' '$1 == "version" { print $2 }')"
commit="$(files_of | awk -F '\t' '$1 == "commit" { print $2 }')"
build_id="${version}+${commit:0:12}"

work="$(mktemp -d)"
upstream_pid=""
server_pid=""
cleanup() {
    [[ -n "${server_pid}" ]] && kill "${server_pid}" 2>/dev/null || true
    [[ -n "${upstream_pid}" ]] && kill "${upstream_pid}" 2>/dev/null || true
    wait 2>/dev/null || true
    rm -rf "${work}"
}
trap cleanup EXIT

# GitHub's release download layout.
served="${work}/upstream"
mkdir -p "${served}/download/v${version}" "${served}/latest/download"
for asset in "${staging}"/*; do
    cp "${asset}" "${served}/download/v${version}/"
    cp "${asset}" "${served}/latest/download/"
done

# GitHub's REST releases list, as the release index reads it: one published
# release, every staged asset uploaded.
node -e '
const [version, dir] = process.argv.slice(1);
const assets = require("node:fs").readdirSync(dir).sort()
    .map((name) => ({ name, state: "uploaded" }));
process.stdout.write(JSON.stringify([{
    tag_name: `v${version}`, draft: false, prerelease: false,
    published_at: "2099-01-01T00:00:00Z", assets,
}]));
' "${version}" "${staging}" >"${served}/releases.json"

upstream_port="$(scripts/dev-port.sh firmware-upstream)"
server_port="$(scripts/dev-port.sh firmware-store-smoke)"
upstream_log="${work}/upstream.log"
python3 -m http.server "${upstream_port}" --bind 127.0.0.1 --directory "${served}" \
    >"${upstream_log}" 2>&1 &
upstream_pid=$!

echo "== building lp-cloud-server"
cargo build -q -p lp-cloud-server
LP_CLOUD_PORT="${server_port}" \
LP_CLOUD_BASE_URL="http://127.0.0.1:${server_port}" \
LP_CLOUD_STORE=mem \
LP_CLOUD_BLOBS=fs \
LP_CLOUD_DATA_DIR="${work}/cloud-data" \
LP_CLOUD_FIRMWARE_UPSTREAM="http://127.0.0.1:${upstream_port}" \
LP_CLOUD_FIRMWARE_RELEASES_LIST="http://127.0.0.1:${upstream_port}/releases.json" \
    cargo run -q -p lp-cloud-server >"${work}/server.log" 2>&1 &
server_pid=$!

base="http://127.0.0.1:${server_port}"
for _ in $(seq 1 120); do
    if curl -fsS --max-time 2 "${base}/healthz" >/dev/null 2>&1 \
        && curl -fsS --max-time 2 "http://127.0.0.1:${upstream_port}/" >/dev/null 2>&1; then
        break
    fi
    sleep 0.5
done
curl -fsS --max-time 2 "${base}/healthz" >/dev/null || {
    echo "firmware-store-smoke: lp-cloud-server never answered; its log:" >&2
    cat "${work}/server.log" >&2
    exit 1
}

passed=0
failed=0
ok() {
    passed=$((passed + 1))
    echo "  ok    $*"
}
bad() {
    failed=$((failed + 1))
    echo "  FAIL  $*"
}
# expect <description> <command...>: ok when the command succeeds.
expect() {
    local what="$1"
    shift
    if "$@"; then
        ok "${what}"
    else
        bad "${what}"
    fi
}

# Upstream requests so far (the http.server log has one line per request).
upstream_requests() {
    grep -c '"GET ' "${upstream_log}" || true
}

sha_of() {
    node "${here}/release-files.mjs" sha256 "$1"
}

# GET <path> into ${work}/body, headers into ${work}/headers; print the
# status code.
get() {
    curl -sS -o "${work}/body" -D "${work}/headers" -w '%{http_code}' "${base}$1"
}

header() {
    grep -i "^$1:" "${work}/headers" | head -n1 | cut -d' ' -f2- | tr -d '\r'
}

echo "== smoke against ${base} (upstream :${upstream_port}) for ${target} ${version}"

# 1. The manifest by version.
path="/firmware/${target}/${version}/ota-manifest.json"
status="$(get "${path}")"
if [[ "${status}" == 200 ]] && cmp -s "${work}/body" "${staging}/${target}.ota-manifest.json"; then
    ok "${path}: 200, byte-equal to the staged asset"
else
    bad "${path}: ${status}"
fi
expect "  ACAO: $(header access-control-allow-origin)" test "$(header access-control-allow-origin)" = "*"
expect "  ETag: $(header etag) = its sha256" test "$(header etag)" = "\"$(sha_of "${work}/body")\""
expect "  Cache-Control: $(header cache-control)" grep -qi '^cache-control:.*immutable' "${work}/headers"

# 2. latest.
path="/firmware/${target}/latest/ota-manifest.json"
status="$(get "${path}")"
if [[ "${status}" == 302 && "$(header location)" == "/firmware/${target}/${version}/ota-manifest.json" ]]; then
    ok "${path}: 302 → $(header location)"
else
    bad "${path}: ${status} → $(header location)"
fi
expect "  Cache-Control: $(header cache-control)" grep -qi '^cache-control:.*max-age=60' "${work}/headers"
expect "  ACAO: $(header access-control-allow-origin)" test "$(header access-control-allow-origin)" = "*"

# 3. A tampered upstream file, asked before it is cached, is a 502.
core_z="${served}/download/v${version}/${target}.core.z"
cp "${core_z}" "${work}/core.z.good"
printf '\x00' | dd of="${core_z}" bs=1 seek=100 conv=notrunc 2>/dev/null
if cmp -s "${core_z}" "${work}/core.z.good"; then
    printf '\x01' | dd of="${core_z}" bs=1 seek=100 conv=notrunc 2>/dev/null
fi
path="/firmware/${target}/${version}/core.z"
status="$(get "${path}")"
expect "${path} tampered upstream: ${status} (502 expected)" test "${status}" = 502
cp "${work}/core.z.good" "${core_z}"

# 4. Every file the manifest names.
while IFS=$'\t' read -r file sha; do
    [[ "${file}" == version || "${file}" == commit ]] && continue
    path="/firmware/${target}/${version}/${file}"
    status="$(get "${path}")"
    if [[ "${status}" == 200 ]] && cmp -s "${work}/body" "${staging}/${target}.${file}" \
        && [[ "$(header etag)" == "\"${sha}\"" ]]; then
        ok "${path}: 200, byte-equal, ETag = sha256"
    else
        bad "${path}: ${status}, ETag $(header etag)"
    fi
done < <(files_of)

# 5. A second engine.bin comes from the cache.
before="$(upstream_requests)"
path="/firmware/${target}/${version}/engine.bin"
status="$(get "${path}")"
after="$(upstream_requests)"
if [[ "${status}" == 200 && "${before}" == "${after}" ]]; then
    ok "${path} again: 200 with no upstream request"
else
    bad "${path} again: ${status}, upstream requests ${before} → ${after}"
fi

# 6. A dev version and a reserved word never reach upstream.
for release in abc1234 stable; do
    before="$(upstream_requests)"
    path="/firmware/${target}/${release}/ota-manifest.json"
    status="$(get "${path}")"
    after="$(upstream_requests)"
    if [[ "${status}" == 404 && "${before}" == "${after}" ]]; then
        ok "${path}: 404 with no upstream request"
    else
        bad "${path}: ${status}, upstream requests ${before} → ${after}"
    fi
done

# 7. The build id names the same bytes.
path="/firmware/${target}/${build_id}/ota-manifest.json"
status="$(get "${path}")"
if [[ "${status}" == 200 ]] && cmp -s "${work}/body" "${staging}/${target}.ota-manifest.json"; then
    ok "${path}: 200, the version's bytes"
else
    bad "${path}: ${status}"
fi

# 8. The release index lists the staged release.
path="/api/v1/firmware/${target}/releases"
status="$(get "${path}")"
if [[ "${status}" == 200 ]] && node -e '
const [file, target, version, commit] = process.argv.slice(1);
const index = JSON.parse(require("node:fs").readFileSync(file, "utf8"));
const entry = index.releases.find((e) => e.version === version);
process.exit(index.format === 1 && index.target === target && entry && entry.commit === commit ? 0 : 1);
' "${work}/body" "${target}" "${version}" "${commit}"; then
    ok "${path}: 200, format 1, lists ${version}"
else
    bad "${path}: ${status} $(head -c 300 "${work}/body")"
fi
expect "  ACAO: $(header access-control-allow-origin)" test "$(header access-control-allow-origin)" = "*"
expect "  Cache-Control: $(header cache-control)" grep -qi '^cache-control:.*max-age=60' "${work}/headers"
expect "  ETag: $(header etag) = its sha256" test "$(header etag)" = "\"$(sha_of "${work}/body")\""

# 9. The index is only under /api/v1/: the old two-segment spelling is the
#    page fallback's, not a firmware answer.
path="/firmware/${target}/releases"
status="$(get "${path}")"
if [[ -z "$(header access-control-allow-origin)" ]] && ! node -e '
const body = require("node:fs").readFileSync(process.argv[1], "utf8");
try { process.exit(JSON.parse(body).format === 1 ? 0 : 1); } catch { process.exit(1); }
' "${work}/body"; then
    ok "${path}: ${status}, not the index"
else
    bad "${path}: ${status} answered like the index"
fi

echo "== firmware-store-smoke: ${passed} passed, ${failed} failed ($(upstream_requests) upstream requests)"
[[ "${failed}" -eq 0 ]]
