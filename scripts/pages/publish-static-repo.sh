#!/usr/bin/env bash
set -euo pipefail

usage() {
    echo "usage: $0 --repo OWNER/REPO --dir PATH [--message MESSAGE] [--dry-run]" >&2
}

target_repo=""
artifact_dir=""
message=""
dry_run=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --repo)
            target_repo="${2:-}"
            shift 2
            ;;
        --dir)
            artifact_dir="${2:-}"
            shift 2
            ;;
        --message)
            message="${2:-}"
            shift 2
            ;;
        --dry-run)
            dry_run=true
            shift
            ;;
        *)
            usage
            exit 2
            ;;
    esac
done

if [[ -z "$target_repo" || -z "$artifact_dir" ]]; then
    usage
    exit 2
fi

if [[ ! -d "$artifact_dir" ]]; then
    echo "artifact directory does not exist: $artifact_dir" >&2
    exit 1
fi

token="${LIGHTPLAYER_PAGES_TOKEN:-${GH_TOKEN:-}}"
if [[ "$dry_run" != true && -z "$token" ]]; then
    echo "LIGHTPLAYER_PAGES_TOKEN or GH_TOKEN is required to publish." >&2
    exit 1
fi

message="${message:-deploy: pages $(date -u +%Y-%m-%dT%H:%M:%SZ)}"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

clone_url="https://github.com/${target_repo}.git"
if [[ -n "$token" ]]; then
    clone_url="https://x-access-token:${token}@github.com/${target_repo}.git"
fi

git clone --depth 1 "$clone_url" "$tmp_dir/site" >/dev/null 2>&1

find "$tmp_dir/site" -mindepth 1 -maxdepth 1 ! -name .git -exec rm -rf {} +
cp -R "${artifact_dir}/." "$tmp_dir/site/"

cd "$tmp_dir/site"
git add -A

if git diff --cached --quiet; then
    echo "No changes to publish for ${target_repo}."
    exit 0
fi

if [[ "$dry_run" == true ]]; then
    echo "Dry run: would publish these changes to ${target_repo}:"
    git status --short
    exit 0
fi

git config user.name "lightplayer-pages-bot"
git config user.email "lightplayer-pages-bot@users.noreply.github.com"
git commit -m "$message"
git push origin HEAD:main
echo "Published ${artifact_dir} to ${target_repo}."
