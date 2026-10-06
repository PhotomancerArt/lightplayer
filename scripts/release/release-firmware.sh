#!/usr/bin/env bash
# Build a release's firmware once, stage it, prove it, and attach it to the
# release — the whole procedure, identical in CI and on a desk.
#
#   scripts/release/release-firmware.sh <version> [--dry-run] [--targets <id,…>]
#                                       [--allow-dev] [--staging <dir>]
#
#   <version>     the release's version (`YYYY.MM.DD-N`; its tag is v<version>).
#                 APP_VERSION must already be exported and equal to it: the
#                 caller resolves the version BEFORE anything writes into the
#                 tree (`print-app-version.sh` reads `git status`), the rule
#                 deploy-cloud.yml documents.
#   --dry-run     package, stage and check, then PRINT the upload and Latest
#                 commands instead of running them (the pre-merge
#                 `release-dry-run` job; nothing touches GitHub).
#   --targets     comma-separated targets (default: every served build,
#                 `just studio-served-builds`).
#   --allow-dev   accept a dev version (`<sha>[-dirty-…]`) — dry runs only.
#   --staging     the staging directory (default target/release-assets/<version>);
#                 it is emptied first.
#
# Steps (plan lp2025/2026-10-04-0757-ota-firmware-distribution, P6):
#   1. package each target (`just studio-firmware-package-target <t> split`);
#      a split target's package step also writes ota-manifest.json + .z;
#   2. stage under the release's asset names (`lp-cli firmware release-assets`,
#      which renames and verifies, never compresses);
#   3. prove the staging directory (`lp-cli firmware release-check`, with
#      --version so every target carries this release's version — one build
#      per release);
#   4. upload: an asset already on the release is compared by SHA-256 —
#      identical is skipped, different FAILS. Release assets are immutable;
#      this never clobbers (D7), which is also what makes a re-run idempotent;
#   5. Latest: mark the newest release that carries firmware for every
#      staged target (D6) — see `mark_latest` for why it is computed rather
#      than "this one".
#   6. print the staged files with sizes, and the release URL.
#
# Needs: just, cargo, node (the recipes), gh (not with --dry-run).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${here}/../.." && pwd)"
# shellcheck source=scripts/release/version-cmp.sh
source "${here}/version-cmp.sh"

usage() {
    sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//' >&2
    exit 2
}

version=""
dry_run=false
allow_dev=false
targets_arg=""
staging=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run) dry_run=true ;;
        --allow-dev) allow_dev=true ;;
        --targets)
            [[ $# -ge 2 ]] || usage
            targets_arg="$2"
            shift
            ;;
        --staging)
            [[ $# -ge 2 ]] || usage
            staging="$2"
            shift
            ;;
        -h | --help) usage ;;
        -*)
            echo "release-firmware: unknown option $1" >&2
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

cd "${repo_root}"

if [[ -z "${APP_VERSION:-}" ]]; then
    echo "release-firmware: APP_VERSION is not exported." >&2
    echo "  Resolve it before anything writes into the tree, then export it:" >&2
    echo "    export APP_VERSION=\"\$(scripts/print-app-version.sh --require-tag)\"" >&2
    exit 1
fi
if [[ "${APP_VERSION}" != "${version}" ]]; then
    echo "release-firmware: APP_VERSION is ${APP_VERSION}, but the release is ${version}" >&2
    exit 1
fi
if ! release_version_fields "${version}" >/dev/null 2>&1 && [[ "${allow_dev}" != true ]]; then
    echo "release-firmware: ${version} is not a release version (--allow-dev for a dry run)" >&2
    exit 1
fi
if [[ "${allow_dev}" == true && "${dry_run}" != true ]]; then
    echo "release-firmware: --allow-dev is for dry runs; a release carries release versions" >&2
    exit 1
fi

if [[ -n "${targets_arg}" ]]; then
    IFS=',' read -r -a targets <<<"${targets_arg}"
else
    targets=()
    while read -r target; do
        targets+=("${target}")
    done < <(just studio-served-builds)
fi
[[ ${#targets[@]} -gt 0 ]] || { echo "release-firmware: no targets" >&2; exit 1; }
targets_csv="$(IFS=','; echo "${targets[*]}")"

staging="${staging:-target/release-assets/${version}}"
tag="v${version}"
lp_cli=(cargo run -q -p lp-cli --)
dry_note=""
[[ "${dry_run}" == true ]] && dry_note=" (dry run)"

echo "== release-firmware ${version} (${targets_csv})${dry_note}"

# 1. Package. Releases always carry the C6's split image and its update files.
for target in "${targets[@]}"; do
    echo "== package ${target}"
    just studio-firmware-package-target "${target}" split
done

# 2. Stage. The staging directory must start empty so nothing stale ships.
rm -rf "${staging}"
mkdir -p "$(dirname "${staging}")"
stage_args=(firmware release-assets --targets "${targets_csv}" --out "${staging}")
check_args=(firmware release-check "${staging}" --targets "${targets_csv}" --version "${version}")
if [[ "${allow_dev}" == true ]]; then
    stage_args+=(--allow-dev)
    check_args+=(--allow-dev)
fi
echo "== stage into ${staging}"
"${lp_cli[@]}" "${stage_args[@]}"

# 3. Prove it, from the files alone.
echo "== check"
"${lp_cli[@]}" "${check_args[@]}"

staged_files=()
while IFS= read -r -d '' file; do
    staged_files+=("${file}")
done < <(find "${staging}" -maxdepth 1 -type f -print0 | sort -z)

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

size_of() {
    wc -c <"$1" | tr -d ' '
}

# 4. Upload, immutably.
upload_assets() {
    local existing
    # name<TAB>state<TAB>digest — `digest` is GitHub's own `sha256:<hex>`.
    existing="$(gh release view "${tag}" --json assets \
        --jq '.assets[] | [.name, .state, (.digest // "")] | @tsv')"
    local file name hash line state digest
    local -a to_upload=()
    for file in "${staged_files[@]}"; do
        name="$(basename "${file}")"
        hash="$(sha256_of "${file}")"
        line="$(awk -F '\t' -v n="${name}" '$1 == n' <<<"${existing}")"
        if [[ -z "${line}" ]]; then
            to_upload+=("${file}")
            continue
        fi
        state="$(cut -f2 <<<"${line}")"
        digest="$(cut -f3 <<<"${line}")"
        if [[ "${state}" != "uploaded" ]]; then
            # An upload that never completed was never served: nothing to
            # be immutable about. Remove it and upload again.
            echo "   ${name}: a previous upload never completed (${state}); replacing it"
            gh release delete-asset "${tag}" "${name}" --yes
            to_upload+=("${file}")
            continue
        fi
        if [[ -z "${digest}" ]]; then
            local tmp
            tmp="$(mktemp -d)"
            gh release download "${tag}" --pattern "${name}" --dir "${tmp}"
            digest="sha256:$(sha256_of "${tmp}/${name}")"
            rm -rf "${tmp}"
        fi
        if [[ "${digest}" == "sha256:${hash}" ]]; then
            echo "   ${name}: already on ${tag}, identical — skipped"
        else
            echo "release-firmware: release assets are immutable: ${name} differs" >&2
            echo "  on ${tag}: ${digest}; staged: sha256:${hash}" >&2
            echo "  (never --clobber: a board's engine hash must keep resolving)" >&2
            return 1
        fi
    done
    if [[ ${#to_upload[@]} -gt 0 ]]; then
        echo "   uploading ${#to_upload[@]} assets to ${tag}"
        gh release upload "${tag}" "${to_upload[@]}"
    fi
}

# 5. Latest = the newest release (by version) that carries a package for
# every staged target. Computed from the releases' assets rather than "this
# one if newer than the current Latest", so two runs finishing at once
# converge: each decides after its own upload, the last one to decide sees
# every finished upload, and an older run finishing late can never move
# Latest backwards. Reads the newest 100 releases (one API call).
mark_latest() {
    local required=() target
    for target in "${targets[@]}"; do
        required+=("${target}.package.json")
    done
    local releases
    # tag<TAB>space-separated uploaded asset names, newest first.
    releases="$(gh api "repos/{owner}/{repo}/releases?per_page=100" \
        --jq '.[] | select((.draft or .prerelease) | not)
              | [.tag_name, ([.assets[] | select(.state == "uploaded") | .name] | join(" "))]
              | @tsv')"
    local -a with_firmware=()
    local rtag assets name ok
    while IFS=$'\t' read -r rtag assets; do
        [[ "${rtag}" == v* ]] || continue
        release_version_fields "${rtag#v}" >/dev/null 2>&1 || continue
        ok=true
        for name in "${required[@]}"; do
            if [[ " ${assets} " != *" ${name} "* ]]; then
                ok=false
                break
            fi
        done
        [[ "${ok}" == true ]] && with_firmware+=("${rtag#v}")
    done <<<"${releases}"
    if [[ ${#with_firmware[@]} -eq 0 ]]; then
        echo "release-firmware: no release carries firmware for ${targets_csv} — not even ${tag}" >&2
        return 1
    fi
    local newest current
    newest="$(release_version_max "${with_firmware[@]}")"
    current="$(gh release list --limit 100 --json tagName,isLatest \
        --jq '.[] | select(.isLatest) | .tagName')"
    if [[ "${current}" == "v${newest}" ]]; then
        echo "   Latest is already v${newest}"
    else
        echo "   marking v${newest} Latest (was ${current:-none})"
        gh release edit "v${newest}" --latest
    fi
}

if [[ "${dry_run}" == true ]]; then
    echo "== dry run: the upload and Latest steps would run:"
    echo "   gh release view ${tag} --json assets   # identical assets skipped, different ones fail"
    for file in "${staged_files[@]}"; do
        echo "   gh release upload ${tag} ${file}"
    done
    echo "   gh release edit v<newest release carrying ${targets_csv}> --latest"
else
    echo "== upload to ${tag}"
    upload_assets
    echo "== latest"
    mark_latest
fi

# 6. Report.
release_url="https://github.com/${GITHUB_REPOSITORY:-PhotomancerArt/lightplayer}/releases/tag/${tag}"
summary="$(
    echo "Release firmware ${version}${dry_note}: ${#staged_files[@]} files"
    echo
    echo "| asset | bytes | sha256 |"
    echo "|---|---:|---|"
    for file in "${staged_files[@]}"; do
        echo "| \`$(basename "${file}")\` | $(size_of "${file}") | \`$(sha256_of "${file}")\` |"
    done
    echo
    echo "${release_url}"
)"
echo "${summary}"
if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    echo "${summary}" >>"${GITHUB_STEP_SUMMARY}"
fi
