#!/usr/bin/env bash
# Release versions (`YYYY.MM.DD-N`, the tag without its `v`) in order.
#
#   scripts/release/version-cmp.sh lt <a> <b>    exit 0 when a sorts before b
#   scripts/release/version-cmp.sh max <v>...    print the newest
#
# Or `source` it for `release_version_lt` / `release_version_max`.
#
# Numeric field by field, never a string sort: `2026.10.05-9` is older than
# `2026.10.05-10`, which is older than `2026.10.06-1`. Anything that is not a
# release version (a dev version `<sha>[-dirty-…]`, a stray tag) is refused,
# so it can never become Latest by sorting. `release-firmware.sh` reads it to
# decide which release GitHub marks Latest (one-way-doors D6).

# Print a release version's four fields, space-separated; fail on anything
# else.
release_version_fields() {
    local v="$1"
    if [[ ! "${v}" =~ ^([0-9]{4})\.([0-9]{2})\.([0-9]{2})-([0-9]+)$ ]]; then
        echo "not a release version (YYYY.MM.DD-N): ${v}" >&2
        return 2
    fi
    # 10# so a leading zero is decimal, not octal.
    echo "$((10#${BASH_REMATCH[1]})) $((10#${BASH_REMATCH[2]})) $((10#${BASH_REMATCH[3]})) $((10#${BASH_REMATCH[4]}))"
}

# Exit 0 when release version $1 sorts strictly before $2; 1 when not; 2
# when either is not a release version.
release_version_lt() {
    local a b
    a="$(release_version_fields "$1")" || return 2
    b="$(release_version_fields "$2")" || return 2
    local -a fa fb
    read -r -a fa <<<"${a}"
    read -r -a fb <<<"${b}"
    local i
    for i in 0 1 2 3; do
        if ((fa[i] < fb[i])); then
            return 0
        fi
        if ((fa[i] > fb[i])); then
            return 1
        fi
    done
    return 1
}

# Print the newest of the release versions given; fail if one is not.
release_version_max() {
    local best="" v
    for v in "$@"; do
        release_version_fields "${v}" >/dev/null || return 2
        if [[ -z "${best}" ]] || release_version_lt "${best}" "${v}"; then
            best="${v}"
        fi
    done
    [[ -n "${best}" ]] || return 2
    echo "${best}"
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    set -euo pipefail
    case "${1:-}" in
        lt)
            [[ $# -eq 3 ]] || { echo "usage: $0 lt <a> <b>" >&2; exit 2; }
            release_version_lt "$2" "$3"
            ;;
        max)
            shift
            release_version_max "$@"
            ;;
        *)
            echo "usage: $0 lt <a> <b> | max <v>..." >&2
            exit 2
            ;;
    esac
fi
