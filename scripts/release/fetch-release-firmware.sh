#!/usr/bin/env bash
# Put a release's firmware where the Studio bundle step expects it, verified
# — so a deploy ships the bytes the release (and so the firmware store)
# carries, never a second build of them (one build, plan D5).
#
#   scripts/release/fetch-release-firmware.sh <version> [--wait-for-sha <sha>]
#                                             [--from-dir <staging>] [--targets <id,…>]
#
#   <version>       the release (`YYYY.MM.DD-N`; its tag is v<version>).
#   --wait-for-sha  if the release does not carry its firmware yet, wait for
#                   that commit's "Release firmware" run (found by its
#                   run-name, retried for up to 5 min while it does not
#                   exist yet), then download. A failed run fails this.
#   --from-dir      read the assets from a staging directory instead of the
#                   release (release-firmware.sh's output; how this is tested
#                   before a release exists).
#   --targets       comma-separated targets (default: every served build).
#
# Writes, per target, what `lp-cli firmware package` would have:
#
#   target/studio-web-assets/firmware/<target>/manifest.json   <target>.package.json
#   target/studio-web-assets/firmware/<target>/<image>          <target>.<image>
#   target/firmware-parts/<target>/ota-manifest.json            <target>.ota-manifest.json  (split only)
#   target/firmware-parts/<target>/core.z, engine.z             <target>.core.z, .engine.z  (split only)
#
# — the inputs `studio-web-copy-sidecars` hands to
# scripts/studio-copy-firmware.sh, which copies the package into the bundle's
# `firmware/<target>/` and a split target's three update files into its
# `firmware/<target>/ota/` (OTA M7, DS10). `core.bin` / `engine.bin` are not
# fetched: Studio slices them out of the merged image.
#
# Every image is checked against the package manifest it came with, and the
# update files against `ota-manifest.json`, whose `package` entry must hash
# this very package manifest — all from one release, or nothing is written.
#
# Needs: node (scripts/release/release-files.mjs); gh (not with --from-dir).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${here}/../.." && pwd)"

usage() {
    sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//' >&2
    exit 2
}

version=""
wait_sha=""
from_dir=""
targets_arg=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --wait-for-sha)
            [[ $# -ge 2 ]] || usage
            wait_sha="$2"
            shift
            ;;
        --from-dir)
            [[ $# -ge 2 ]] || usage
            from_dir="$(cd "$2" && pwd)"
            shift
            ;;
        --targets)
            [[ $# -ge 2 ]] || usage
            targets_arg="$2"
            shift
            ;;
        -h | --help) usage ;;
        -*)
            echo "fetch-release-firmware: unknown option $1" >&2
            usage
            ;;
        *)
            [[ -z "${version}" ]] || usage
            version="$1"
            ;;
    esac
    shift
done
[[ -n "${version}" ]] || usage
tag="v${version}"

cd "${repo_root}"

if [[ -n "${targets_arg}" ]]; then
    IFS=',' read -r -a targets <<<"${targets_arg}"
else
    targets=()
    while read -r target; do
        targets+=("${target}")
    done < <(node -e 'console.log(JSON.parse(require("fs").readFileSync("lp-fw/builds/served.json","utf8")).builds.join("\n"))')
fi
[[ ${#targets[@]} -gt 0 ]] || { echo "fetch-release-firmware: no targets" >&2; exit 1; }

assets_root="target/studio-web-assets/firmware"
parts_root="target/firmware-parts"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

# The release's uploaded asset names, one per line.
release_assets() {
    gh release view "${tag}" --json assets \
        --jq '.assets[] | select(.state == "uploaded") | .name'
}

# Does the release carry every target's package yet?
release_has_firmware() {
    local names target
    names="$(release_assets 2>/dev/null)" || return 1
    for target in "${targets[@]}"; do
        grep -qxF "${target}.package.json" <<<"${names}" || return 1
    done
}

# Wait for this sha's "Release firmware" run (its run-name is
# "Release firmware <sha>"), then for the assets it uploads.
wait_for_release_run() {
    local run_id="" attempt
    for attempt in $(seq 1 30); do
        run_id="$(gh run list --workflow release-firmware.yml --limit 100 \
            --json databaseId,displayTitle \
            --jq "[.[] | select(.displayTitle == \"Release firmware ${wait_sha}\" or .displayTitle == \"Release firmware ${tag}\")][0].databaseId // empty")" \
            || run_id="" # a lookup that errors is retried like one that finds nothing
        [[ -n "${run_id}" ]] && break
        echo "   no \"Release firmware\" run for ${wait_sha} yet (${attempt}/30); retrying in 10 s"
        sleep 10
    done
    if [[ -z "${run_id}" ]]; then
        echo "fetch-release-firmware: no \"Release firmware\" run for ${wait_sha} after 5 min" >&2
        return 1
    fi
    echo "   waiting for Release firmware run ${run_id}"
    if ! gh run watch "${run_id}" --exit-status --interval 30 >/dev/null; then
        echo "fetch-release-firmware: Release firmware failed for ${wait_sha}; the deploy cannot ship firmware it did not build" >&2
        echo "  run: $(gh run view "${run_id}" --json url --jq .url)" >&2
        return 1
    fi
}

# Copy (or download) one asset into ${work}.
get_asset() {
    local name="$1"
    if [[ -n "${from_dir}" ]]; then
        [[ -f "${from_dir}/${name}" ]] || {
            echo "fetch-release-firmware: ${from_dir}/${name} is missing" >&2
            return 1
        }
        cp "${from_dir}/${name}" "${work}/${name}"
    else
        gh release download "${tag}" --pattern "${name}" --dir "${work}" --clobber
    fi
}

if [[ -n "${from_dir}" ]]; then
    echo "== firmware for ${tag} from ${from_dir}"
else
    echo "== firmware for ${tag} from its release"
    if ! release_has_firmware; then
        if [[ -z "${wait_sha}" ]]; then
            echo "fetch-release-firmware: ${tag} does not carry firmware for ${targets[*]}" >&2
            exit 1
        fi
        wait_for_release_run
        release_has_firmware || {
            echo "fetch-release-firmware: Release firmware finished, but ${tag} still lacks firmware for ${targets[*]}" >&2
            exit 1
        }
    fi
fi

# Fetch and verify everything first; write the tree only once all of it
# checks, so a failure never leaves half a release beside a local build.
files=(node "${here}/release-files.mjs")
declare -a plan=()
for target in "${targets[@]}"; do
    get_asset "${target}.package.json"
    # One line per image ("image<TAB>file<TAB>size<TAB>sha256"), then
    # "split<TAB>yes|no".
    listing="$("${files[@]}" package "${work}/${target}.package.json" "${target}" "${version}")"
    while IFS=$'\t' read -r kind a b c; do
        case "${kind}" in
            image)
                get_asset "${target}.${a}"
                "${files[@]}" image "${work}/${target}.${a}" "${b}" "${c}"
                plan+=("${work}/${target}.${a}|${assets_root}/${target}/${a}")
                ;;
            split)
                [[ "${a}" == "yes" ]] || continue
                for file in ota-manifest.json core.z engine.z; do
                    get_asset "${target}.${file}"
                done
                "${files[@]}" ota "${work}" "${target}"
                for file in ota-manifest.json core.z engine.z; do
                    plan+=("${work}/${target}.${file}|${parts_root}/${target}/${file}")
                done
                ;;
        esac
    done <<<"${listing}"
    plan+=("${work}/${target}.package.json|${assets_root}/${target}/manifest.json")
done

# Write. Each target's package directory is replaced whole (the bundle step
# copies every `*.bin` in it, so nothing stale may stay); in the parts
# directory only the three update files are replaced.
for target in "${targets[@]}"; do
    rm -rf "${assets_root:?}/${target}"
    mkdir -p "${assets_root}/${target}" "${parts_root}/${target}"
    rm -f "${parts_root}/${target}/ota-manifest.json" "${parts_root}/${target}/core.z" \
        "${parts_root}/${target}/engine.z"
done
for entry in "${plan[@]}"; do
    cp "${entry%%|*}" "${entry##*|}"
done

summary="Firmware: release ${tag}"
for target in "${targets[@]}"; do
    for image in "${assets_root}/${target}"/*.bin; do
        sha="$("${files[@]}" sha256 "${image}")"
        summary+=$'\n'"- ${target}: $(basename "${image}") sha256 ${sha}"
    done
done
echo "${summary}"
if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    echo "${summary}" >>"${GITHUB_STEP_SUMMARY}"
fi
