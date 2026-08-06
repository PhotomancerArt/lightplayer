#!/usr/bin/env bash
# One-time (but re-runnable) creation of the fly.io resources behind
# lightplayer.app, ending in the exact DNS rows for the GoDaddy cutover.
#
#   ./infra/bootstrap.sh
#
# Every step is create-if-absent: running this twice changes nothing and
# still prints the DNS block at the end. That property is the point — it is
# also how you re-read the values without hunting through fly's dashboard.
#
# What it does NOT do: deploy (that is CI or `flyctl deploy`), touch DNS
# (that is docs/runbooks/godaddy-dns-cutover.md, Yona's hands, his GoDaddy
# account), or print a secret. Secrets are read from the environment or a
# silent prompt and piped straight into `fly secrets import` — never passed
# as arguments, because argv is world-readable in `ps`.
set -euo pipefail

# --- what we are building ---------------------------------------------------
APP="${LP_FLY_APP:-lightplayer}"
REGION="${LP_FLY_REGION:-sjc}"  # sea no longer exists for new orgs
VOLUME="${LP_FLY_VOLUME:-data}"
VOLUME_GB="${LP_FLY_VOLUME_GB:-3}"
BUCKET="${LP_FLY_BUCKET:-lightplayer-cloud}"
DOMAIN="${LP_DOMAIN:-lightplayer.app}"
ORG="${LP_FLY_ORG:-personal}"

# Where the Tigris credentials are stashed for a human. NOT in the repo, not
# in the terminal: `fly storage create` shows the secret access key exactly
# once, and you will want it the day you run `litestream restore` from a
# laptop against the bucket.
CREDS_DIR="${LP_CREDS_DIR:-$HOME/.lightplayer}"
CREDS_FILE="${CREDS_DIR}/tigris-${APP}.env"

# --- output helpers ---------------------------------------------------------
step()  { printf '\n\033[1m==> %s\033[0m\n' "$*"; }
info()  { printf '    %s\n' "$*"; }
skip()  { printf '    \033[2m· %s\033[0m\n' "$*"; }
die()   { printf '\n\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# 0. Preconditions
# ---------------------------------------------------------------------------
step "Preconditions"

command -v fly >/dev/null 2>&1 || die "fly CLI not found — https://fly.io/docs/flyctl/install/"
# jq parses `fly ips list --json`. The plain-text table has changed shape
# between flyctl releases; the JSON has not.
command -v jq >/dev/null 2>&1 || die "jq not found — brew install jq"

if ! WHO="$(fly auth whoami 2>/dev/null)"; then
    die "not logged in — run: fly auth login"
fi
info "fly account: ${WHO}"
info "app ${APP} · region ${REGION} · org ${ORG} · domain ${DOMAIN}"

# ---------------------------------------------------------------------------
# 1. The app
# ---------------------------------------------------------------------------
step "App"

if fly status --app "$APP" >/dev/null 2>&1; then
    skip "app ${APP} already exists"
else
    info "creating app ${APP}"
    fly apps create "$APP" --org "$ORG"
fi

# ---------------------------------------------------------------------------
# 2. The volume
# ---------------------------------------------------------------------------
# One volume, deliberately: it is what pins the service to a single machine,
# which is what makes single-writer SQLite correct. See infra/fly.toml.
step "Volume"

if fly volumes list --app "$APP" 2>/dev/null | grep -qw "$VOLUME"; then
    skip "volume ${VOLUME} already exists (fly volumes list --app ${APP})"
else
    info "creating ${VOLUME_GB}GB volume '${VOLUME}' in ${REGION}"
    fly volumes create "$VOLUME" \
        --app "$APP" \
        --region "$REGION" \
        --size "$VOLUME_GB" \
        --yes
fi

# ---------------------------------------------------------------------------
# 3. Tigris object storage
# ---------------------------------------------------------------------------
# `fly storage create` provisions the bucket AND sets AWS_ACCESS_KEY_ID /
# AWS_SECRET_ACCESS_KEY (plus BUCKET_NAME / AWS_ENDPOINT_URL_S3 / AWS_REGION)
# as secrets on the app. Those same two variables are read by the blob store
# and by litestream — one credential, one rotation.
step "Object storage (Tigris)"

if fly secrets list --app "$APP" 2>/dev/null | grep -q "AWS_ACCESS_KEY_ID"; then
    skip "AWS_ACCESS_KEY_ID already set on ${APP} — assuming bucket ${BUCKET} exists"
    skip "to re-create: fly storage list, then destroy and re-run"
else
    mkdir -p "$CREDS_DIR"
    chmod 700 "$CREDS_DIR"
    umask 077

    info "creating bucket ${BUCKET} (credentials go to ${CREDS_FILE}, not to this terminal)"
    info "note: --yes accepts the Tigris terms of service on your behalf"
    # Output is captured, never echoed: it contains the secret access key.
    # `|| true` is NOT used — a failure here must stop the script, and the
    # captured output is dumped to the creds file first so the failure is
    # diagnosable without re-running a half-successful create.
    # --yes: with stdout captured this runs non-interactively, and the
    # Tigris provisioning includes a terms-of-service agreement that
    # otherwise dies with "the --yes flag must be specified".
    if fly storage create --name "$BUCKET" --app "$APP" --org "$ORG" --yes >"$CREDS_FILE" 2>&1; then
        chmod 600 "$CREDS_FILE"
        info "bucket created; credentials saved to ${CREDS_FILE} (mode 600)"
        info "copy them into your password manager, then: rm ${CREDS_FILE}"
    else
        chmod 600 "$CREDS_FILE"
        die "fly storage create failed — its output (may contain secrets) is in ${CREDS_FILE}"
    fi
fi

# ---------------------------------------------------------------------------
# 4. Application secrets
# ---------------------------------------------------------------------------
# Google OAuth credentials, from the console (see infra/README.md, "The two
# manual consoles"). Taken from the environment if present so this script can
# run unattended; otherwise prompted with echo off.
step "Secrets"

read_secret() {
    # $1 = variable name to read from the environment, $2 = prompt
    local from_env="${!1:-}"
    if [[ -n "$from_env" ]]; then
        printf '%s' "$from_env"
        return
    fi
    local value=""
    # Prompt on stderr: stdout is the value's channel.
    read -r -s -p "    $2: " value </dev/tty >&2 || die "no tty for the prompt — set $1 in the environment instead"
    printf '\n' >&2
    printf '%s' "$value"
}

if fly secrets list --app "$APP" 2>/dev/null | grep -q "LP_CLOUD_GOOGLE_CLIENT_SECRET"; then
    skip "LP_CLOUD_GOOGLE_CLIENT_SECRET already set — leaving it alone"
    skip "to rotate: fly secrets import --app ${APP}  (then paste KEY=VALUE, ^D)"
else
    info "Google OAuth client, from console.cloud.google.com (see infra/README.md)"
    GOOGLE_ID="$(read_secret LP_CLOUD_GOOGLE_CLIENT_ID 'Google client id')"
    GOOGLE_SECRET="$(read_secret LP_CLOUD_GOOGLE_CLIENT_SECRET 'Google client secret')"

    [[ -n "$GOOGLE_ID" && -n "$GOOGLE_SECRET" ]] || die "both Google values are required"

    # `import` reads KEY=VALUE from stdin. `secrets set K=V` would put the
    # secret in this process's argv, visible to every user on the machine.
    printf 'LP_CLOUD_GOOGLE_CLIENT_ID=%s\nLP_CLOUD_GOOGLE_CLIENT_SECRET=%s\n' \
        "$GOOGLE_ID" "$GOOGLE_SECRET" \
        | fly secrets import --app "$APP"

    unset GOOGLE_ID GOOGLE_SECRET
    info "set (values never printed)"
fi

# ---------------------------------------------------------------------------
# 5. IP addresses
# ---------------------------------------------------------------------------
# Shared IPv4 (free; fly's edge routes it by SNI/Host, which is all an apex A
# record needs) + a dedicated IPv6.
step "IP addresses"

ips_json() { fly ips list --app "$APP" --json 2>/dev/null || echo '[]'; }

# flyctl has shipped both `Address`/`Type` and `address`/`type` spellings.
# Ask for either rather than pinning a version.
ip_of() {
    ips_json | jq -r --arg want "$1" \
        '.[] | select((.Type // .type) == $want) | (.Address // .address)' \
        | head -n1
}

if [[ -n "$(ip_of v4)" ]]; then
    skip "IPv4 already allocated"
else
    info "allocating shared IPv4"
    fly ips allocate-v4 --shared --app "$APP" --yes
fi

if [[ -n "$(ip_of v6)" ]]; then
    skip "IPv6 already allocated"
else
    info "allocating IPv6"
    fly ips allocate-v6 --app "$APP"
fi

IPV4="$(ip_of v4)"
IPV6="$(ip_of v6)"

# ---------------------------------------------------------------------------
# 6. TLS certificate — apex only
# ---------------------------------------------------------------------------
# No www. Q20 was overridden deliberately: "I don't think anyone uses
# www.lightplayer.app — we have no users, so now's the time to break it."
# The runbook DELETES the www CNAME; adding a cert for it here would quietly
# resurrect the thing we decided to kill.
step "TLS certificate"

if fly certs list --app "$APP" 2>/dev/null | grep -q "$DOMAIN"; then
    skip "certificate for ${DOMAIN} already requested"
else
    info "requesting certificate for ${DOMAIN} (apex only — www dies by design)"
    fly certs add "$DOMAIN" --app "$APP"
fi

# ---------------------------------------------------------------------------
# 7. The DNS block
# ---------------------------------------------------------------------------
step "DNS — the rows to enter at GoDaddy"

if [[ -z "$IPV4" || -z "$IPV6" ]]; then
    info "could not read one of the addresses back from flyctl."
    info "run: fly ips list --app ${APP}"
    IPV4="${IPV4:-<see fly ips list>}"
    IPV6="${IPV6:-<see fly ips list>}"
fi

cat <<BLOCK

  Enter these at dcc.godaddy.com/control/portfolio/${DOMAIN}/settings

     Type   Name   Value                              TTL
     ----   ----   --------------------------------   ---
     A      @      ${IPV4}   600
     AAAA   @      ${IPV6}   600

  DELETE first, in the same sitting (all eight are GitHub Pages):
     A     @    185.199.108.153
     A     @    185.199.109.153
     A     @    185.199.110.153
     A     @    185.199.111.153
     AAAA  @    2606:50c0:8000::153
     AAAA  @    2606:50c0:8001::153
     AAAA  @    2606:50c0:8002::153
     AAAA  @    2606:50c0:8003::153
  And DELETE, without replacing it:
     CNAME www  light-player.github.io

  Full procedure, verification commands, and rollback:
     docs/runbooks/godaddy-dns-cutover.md

  Before touching DNS, finish the fly.dev smoke checklist (P11):
     fly certs show ${DOMAIN} --app ${APP}     # expect: Awaiting configuration
     curl -sI https://${APP}.fly.dev/healthz   # expect: HTTP/2 200

BLOCK

step "Done"
info "nothing here deploys — next: flyctl deploy . --config infra/fly.toml --dockerfile infra/Dockerfile --remote-only"
