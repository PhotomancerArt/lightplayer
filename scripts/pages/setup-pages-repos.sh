#!/usr/bin/env bash
set -euo pipefail

owner="${LIGHTPLAYER_PAGES_OWNER:-light-player}"
source_repo="${LIGHTPLAYER_SOURCE_REPO:-lightplayer}"
beta_repo="${LIGHTPLAYER_BETA_PAGES_REPO:-lightplayer-beta-pages}"
demo_repo="${LIGHTPLAYER_DEMO_PAGES_REPO:-lightplayer-demo-pages}"
apply=false

for arg in "$@"; do
    case "$arg" in
        --apply) apply=true ;;
        --dry-run) apply=false ;;
        *)
            echo "usage: $0 [--apply|--dry-run]" >&2
            exit 2
            ;;
    esac
done

run() {
    printf '+'
    printf ' %q' "$@"
    printf '\n'
    if [[ "$apply" == true ]]; then
        "$@"
    fi
}

repo_exists() {
    gh repo view "$1" >/dev/null 2>&1
}

ensure_repo() {
    local repo="$1"
    local description="$2"
    if repo_exists "${owner}/${repo}"; then
        echo "Repository exists: ${owner}/${repo}"
    else
        run gh repo create "${owner}/${repo}" --public --add-readme --description "$description"
    fi
}

ensure_branch_pages() {
    local repo="$1"
    local domain="$2"
    if [[ "$apply" != true ]]; then
        echo "Would configure Pages for ${owner}/${repo}: branch main /, custom domain ${domain}"
        return
    fi

    if gh api "repos/${owner}/${repo}/pages" >/dev/null 2>&1; then
        gh api -X PUT "repos/${owner}/${repo}/pages" \
            -f cname="$domain" \
            -f source[branch]=main \
            -f source[path]=/ >/dev/null
    else
        gh api -X POST "repos/${owner}/${repo}/pages" \
            -f source[branch]=main \
            -f source[path]=/ >/dev/null
        gh api -X PUT "repos/${owner}/${repo}/pages" \
            -f cname="$domain" >/dev/null
    fi
}

if ! command -v gh >/dev/null 2>&1; then
    echo "gh is required." >&2
    exit 1
fi

echo "Mode: $([[ "$apply" == true ]] && echo apply || echo dry-run)"
echo "Owner: ${owner}"

ensure_repo "$beta_repo" "LightPlayer beta Studio GitHub Pages site"
ensure_repo "$demo_repo" "LightPlayer web demo GitHub Pages site"
ensure_branch_pages "$beta_repo" "beta.lightplayer.app"
ensure_branch_pages "$demo_repo" "demo.lightplayer.app"

cat <<EOF

Manual checks still required:
- In https://github.com/${owner}/${source_repo}/settings/pages, set Pages source to GitHub Actions.
- Set the production custom domain to lightplayer.app.
- Add LIGHTPLAYER_PAGES_APP_ID and LIGHTPLAYER_PAGES_PRIVATE_KEY as repository
  secrets in ${owner}/${source_repo}. The app needs Contents: Read and write
  permission and should be installed only on ${owner}/${beta_repo} and
  ${owner}/${demo_repo}.
- Confirm DNS records for lightplayer.app, www.lightplayer.app, beta.lightplayer.app, and demo.lightplayer.app.
- Wait for GitHub Pages DNS checks and TLS certificates, then enable Enforce HTTPS.
EOF
