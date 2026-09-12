#!/usr/bin/env bash
# The director's CLI over the emulator perf lab (scripts/emu/lab/server.mjs).
#
#   lab.sh status                                  # devices, builds, job counts
#   lab.sh devices                                 # the device table
#   lab.sh queue --build 23a3d3c --rows gate-rows --repeats 3 --spacing 3m [--device NAME] [--ttl 24h] [--note ...]
#   lab.sh queue --ab 86cb2e0 23a3d3c --rows gate-rows --repeats 5 --spacing 3m     # A1 B1 A2 B2 … on one device
#   lab.sh queue --build X --row render-basic:t2:jit:8 --row render-basic:t2:interp  # explicit rows
#   lab.sh wait --job ID [--max-time 3600]         # ONE blocking call; prints report.md's path on exit 0
#   lab.sh wait --device any|NAME | --queue-idle   # until a device is present / the queue drains
#   lab.sh report ID                               # cat report.md
#   lab.sh jobs                                    # table of jobs and states
#   lab.sh cancel ID
#   lab.sh home | token | curl /status [curl args] # LAB_HOME; the token FILE's path; authenticated passthrough
#   lab.sh install [--force]                       # the two launchd agents (server + tunnel); prints the bookmark
#   lab.sh uninstall | restart [server|tunnel]     # bootout both / kickstart one or both
#   lab.sh url                                     # the bookmark: the static domain from config.json, else the live random URL
#   lab.sh logs [-n 100] [server|tunnel]           # tail the logs
#
# Talks to http://127.0.0.1:<port> — the director is on the desk, never through
# the tunnel. The port comes from $LAB_HOME/config.json, the token from
# $LAB_HOME/token (0600). LAB_HOME defaults to ~/.photomancer/emu-lab (D6).
#
# `wait` is the dont-poll rule made concrete: run it ONCE in a background task
# whose exit notifies; never in a loop. It exits 0 with the value on stdout
# when the condition holds, 8 on the server's 408 timeout, 22 on any other
# HTTP error (curl -f).
#
# stderr is for chatter, stdout is for the value: `queue` prints a job id,
# `wait --job` prints a report path, `curl` prints the body.
set -euo pipefail

home="${LAB_HOME:-$HOME/.photomancer/emu-lab}"

usage() { sed -n '2,29p' "$0"; }

need_home() {
    [[ -f "$home/config.json" && -f "$home/token" ]] || {
        echo "lab: no lab at $home (start the server once: node scripts/emu/lab/server.mjs)" >&2
        exit 1
    }
}

port() { jq -r '.port // 41111' "$home/config.json"; }
token() { tr -d '\n' <"$home/token"; }
base() { echo "http://127.0.0.1:$(port)"; }

api() {
    local p="$1"; shift
    curl -sS -H "Authorization: Bearer $(token)" "$@" "$(base)$p"
}

# `90s`, `3m`, `24h`, `1500ms`, or a bare number of seconds → milliseconds.
dur_ms() {
    local v="$1"
    case "$v" in
        *ms) echo $(( ${v%ms} )) ;;
        *s) echo $(( ${v%s} * 1000 )) ;;
        *m) echo $(( ${v%m} * 60000 )) ;;
        *h) echo $(( ${v%h} * 3600000 )) ;;
        ''|*[!0-9]*) echo "lab: bad duration '$v' (use 90s, 3m, 24h)" >&2; exit 2 ;;
        *) echo $(( v * 1000 )) ;;
    esac
}

cmd_queue() {
    local builds='[]' rows='"gate-rows"' rowlist='[]' repeats=1 spacing=0 device=any ttl=86400000 retry=1 note=null
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --build) builds="$(jq -cn --arg b "$2" '[$b]')"; shift 2 ;;
            --ab) builds="$(jq -cn --arg a "$2" --arg b "$3" '[$a, $b]')"; shift 3 ;;
            --rows) [[ "$2" == gate-rows ]] || { echo "lab: --rows takes gate-rows (use --row for explicit rows)" >&2; exit 2; }; rows='"gate-rows"'; shift 2 ;;
            --row)
                # slug:grade:mode[:fnBlocks][:timeout]
                IFS=: read -r slug grade mode fn timeout <<<"$2"
                rowlist="$(jq -c --arg slug "$slug" --arg grade "$grade" --arg mode "$mode" --arg fn "${fn:-}" --arg timeout "${timeout:-5500ms}" \
                    '. + [{slug: $slug, grade: $grade, mode: $mode, fnBlocks: (if $fn == "" then null else ($fn|tonumber) end), timeout: $timeout}]' <<<"$rowlist")"
                shift 2 ;;
            --repeats) repeats="$2"; shift 2 ;;
            --spacing) spacing="$(dur_ms "$2")"; shift 2 ;;
            --device) device="$2"; shift 2 ;;
            --ttl) ttl="$(dur_ms "$2")"; shift 2 ;;
            --retry-tainted) retry="$2"; shift 2 ;;
            --note) note="$(jq -cn --arg n "$2" '$n')"; shift 2 ;;
            *) echo "lab: queue: unknown option $1" >&2; exit 2 ;;
        esac
    done
    [[ "$builds" != '[]' ]] || { echo "lab: queue needs --build X or --ab A B" >&2; exit 2; }
    if [[ "$rowlist" != '[]' ]]; then rows="$rowlist"; fi
    local body
    body="$(jq -cn --argjson builds "$builds" --argjson rows "$rows" --argjson repeats "$repeats" --argjson spacingMs "$spacing" \
        --arg device "$device" --argjson ttlMs "$ttl" --argjson retryTainted "$retry" --argjson note "$note" \
        '{kind: "bench", builds: $builds, rows: $rows, repeats: $repeats, spacingMs: $spacingMs, device: $device, ttlMs: $ttlMs, retryTainted: $retryTainted, note: $note}')"
    local resp
    resp="$(api /jobs -H 'Content-Type: application/json' -w '\n%{http_code}' -d "$body")"
    local code="${resp##*$'\n'}"; resp="${resp%$'\n'*}"
    if [[ "$code" != 201 ]]; then echo "lab: queue refused ($code): $(jq -r '.error // .' <<<"$resp")" >&2; exit 1; fi
    jq -r '"lab: queued \(.id): \(.builds|join(" vs ")) × \(.repeats) (\(.presses|length) presses), spacing \(.spacingMs/1000)s, device \(.device), ttl \(.ttlMs/3600000)h\(if .note then " — " + .note else "" end)"' <<<"$resp" >&2
    jq -r .id <<<"$resp"
}

cmd_wait() {
    local q="" max=3600
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --job) q="job=$2"; shift 2 ;;
            --device) q="device=$2"; shift 2 ;;
            --queue-idle) q="queue=idle"; shift ;;
            --max-time) max="$2"; shift 2 ;;
            *) echo "lab: wait: unknown option $1" >&2; exit 2 ;;
        esac
    done
    [[ -n "$q" ]] || { echo "lab: wait needs --job ID, --device any|NAME, or --queue-idle" >&2; exit 2; }
    local resp code
    # The server caps a wait at 3600 s; --max-time is a little longer so the
    # 408 is the server's, with the job state in it, not curl's 28.
    resp="$(curl -sS -H "Authorization: Bearer $(token)" --max-time $(( max + 30 )) -w '\n%{http_code}' "$(base)/wait?$q&timeout=$max")"
    code="${resp##*$'\n'}"; resp="${resp%$'\n'*}"
    case "$code" in
        200)
            case "$q" in
                job=*) jq -r '"lab: job \(.job.id) \(.job.state): presses \(.job.presses.done)/\(.job.presses.total), tainted \(.job.presses.tainted), failed \(.job.presses.failed)"' <<<"$resp" >&2
                       jq -r .reportMd <<<"$resp" ;;
                device=*) jq -r '"lab: device present: \(.device.id) \(.device.name // "-") (\(.device.ua // "?"))"' <<<"$resp" >&2; jq -r .device.id <<<"$resp" ;;
                *) jq -r '"lab: queue idle (\(.jobs.done) done)"' <<<"$resp" >&2 ;;
            esac ;;
        408) echo "lab: wait timed out after ${max}s: $(jq -c . <<<"$resp")" >&2; exit 8 ;;
        *) echo "lab: wait failed ($code): $resp" >&2; exit 22 ;;
    esac
}

# --- the standing service (D7, T8): two launchd user agents ---------------------
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
agents_dir="$HOME/Library/LaunchAgents"
label_server="com.yona.emu-lab"
label_tunnel="com.yona.emu-lab-tunnel"

domain() { jq -r '.domain // empty' "$home/config.json"; }

# The bookmark, with the token in the fragment (D17). The static domain from
# config.json when there is one; otherwise the live random URL off ngrok's
# local API (4040), which changes every time the tunnel restarts.
bookmark() {
    local d; d="$(domain)"
    if [[ -n "$d" ]]; then echo "https://$d/#t=$(token)"; return; fi
    local u
    u="$(curl -s http://127.0.0.1:4040/api/tunnels | jq -r '.tunnels[] | select(.proto == "https") | .public_url' 2>/dev/null | head -1 || true)"
    [[ -n "$u" ]] || { echo "lab: no static domain in $home/config.json and no tunnel answering on :4040 (is the tunnel agent running? lab.sh logs tunnel)" >&2; return 1; }
    echo "$u/#t=$(token)"
}

render() {
    local tmpl="$1" urlargs=""
    local d; d="$(domain)"
    [[ -z "$d" ]] || urlargs="<string>--url</string><string>https://$d</string>"
    sed -e "s|@HOME@|$HOME|g" -e "s|@REPO@|$repo|g" -e "s|@PORT@|$(port)|g" -e "s|@URLARGS@|$urlargs|g" "$tmpl"
}

cmd_install() {
    local force=0
    [[ "${1:-}" == --force ]] && force=1
    need_home
    # @REPO@ is a trap: a worktree is harness-pruned, and an agent pointing
    # at one dies with it (T3 all over again). Install from the primary
    # checkout; --force is for a gate held before the PR merges.
    if [[ "$repo" == */.claude/worktrees/* && $force -eq 0 ]]; then
        echo "lab: refusing to install from a worktree ($repo): the agent would die when the harness prunes it." >&2
        echo "lab: run this from the primary checkout (/Users/yona/dev/photomancer/lp2025), or --force for a gate and re-install after the merge." >&2
        exit 2
    fi
    command -v /opt/homebrew/bin/node >/dev/null || { echo "lab: /opt/homebrew/bin/node missing (D13)" >&2; exit 1; }
    command -v /opt/homebrew/bin/ngrok >/dev/null || { echo "lab: /opt/homebrew/bin/ngrok missing" >&2; exit 1; }
    /opt/homebrew/bin/ngrok config check >/dev/null 2>&1 || { echo "lab: ngrok config check failed (no authtoken? run: ngrok config add-authtoken …)" >&2; exit 1; }
    mkdir -p "$agents_dir" "$home/log"
    render "$here/launchd/$label_server.plist.tmpl" >"$agents_dir/$label_server.plist"
    render "$here/launchd/$label_tunnel.plist.tmpl" >"$agents_dir/$label_tunnel.plist"
    plutil -lint "$agents_dir/$label_server.plist" "$agents_dir/$label_tunnel.plist" >/dev/null
    # A hand-run server on the port would keep the agent crash-looping.
    if lsof -nP -iTCP:"$(port)" -sTCP:LISTEN >/dev/null 2>&1 && ! launchctl print "gui/$(id -u)/$label_server" >/dev/null 2>&1; then
        echo "lab: something else is listening on :$(port) (a hand-run server?) — stop it first" >&2; exit 1
    fi
    for l in "$label_server" "$label_tunnel"; do
        launchctl bootout "gui/$(id -u)/$l" >/dev/null 2>&1 || true
        launchctl bootstrap "gui/$(id -u)" "$agents_dir/$l.plist"
    done
    sleep 2
    for l in "$label_server" "$label_tunnel"; do
        launchctl print "gui/$(id -u)/$l" | grep -E "^\s+(state|pid) " | tr -s ' ' | sed "s|^|lab: $l|" >&2
    done
    curl -sf "http://127.0.0.1:$(port)/healthz" >/dev/null || { echo "lab: server agent is not answering /healthz yet (lab.sh logs server)" >&2; exit 1; }
    echo "lab: installed $agents_dir/$label_server.plist and $label_tunnel.plist (repo $repo$([[ $force -eq 1 ]] && echo ', --force'))" >&2
    [[ -n "$(domain)" ]] || echo "lab: no static domain in $home/config.json — the tunnel URL is random and changes on restart; claim one in the ngrok dashboard (Domains → New Domain), put it in config.json as \"domain\", and re-run install" >&2
    sleep 2
    echo "lab: bookmark this (the token is in the fragment; do not paste it into logs that persist):" >&2
    bookmark
}

cmd_uninstall() {
    for l in "$label_server" "$label_tunnel"; do
        launchctl bootout "gui/$(id -u)/$l" >/dev/null 2>&1 && echo "lab: $l stopped" >&2 || true
        rm -f "$agents_dir/$l.plist"
    done
    echo "lab: agents removed; $home left alone" >&2
}

cmd_restart() {
    local which="${1:-both}"
    for l in "$label_server" "$label_tunnel"; do
        case "$which:$l" in both:*|server:$label_server|tunnel:$label_tunnel) launchctl kickstart -k "gui/$(id -u)/$l" && echo "lab: $l restarted" >&2 ;; esac
    done
}

cmd_logs() {
    local n=100 which=server
    while [[ $# -gt 0 ]]; do
        case "$1" in -n) n="$2"; shift 2 ;; server|tunnel) which="$1"; shift ;; *) echo "lab: logs: unknown $1" >&2; exit 2 ;; esac
    done
    tail -n "$n" "$home/log/$which.log"
}

cmd="${1:-}"; [[ $# -gt 0 ]] && shift
case "$cmd" in
    install) cmd_install "$@" ;;
    uninstall) cmd_uninstall ;;
    restart) cmd_restart "$@" ;;
    url) need_home; bookmark ;;
    logs) need_home; cmd_logs "$@" ;;
    home) echo "$home" ;;
    token) need_home; echo "$home/token" ;;
    status)
        need_home
        api /status | jq -r '
            "lab \(.home) :\(.port) up \(.uptimeS)s · cooldown \(.config.cooldownMs/1000)s · builds \(.builds|length) · jobs queued \(.jobs.queued) running \(.jobs.running) done \(.jobs.done) · results \(.results) · bad tokens \(.tokenFailures)",
            (.devices[] | "device \(.id) \(.name // "-")  \(if .present then "PRESENT" else "away" end)  vis=\(.lastState.visibility // "?") lock=\(.lastState.wakeLock // "?")  seen \(.lastSeen // "-")"),
            (.builds[] | "build  \(.id)  \(.branch)\(if .dirty then " (dirty)" else "" end)  built \(.built_at)")' ;;
    devices)
        need_home
        api /status | jq -r '
            ["id","name","present","visibility","focus","lock","cores","lastSeen"], (.devices[] | [.id, (.name // "-"), (.present|tostring), (.lastState.visibility // "-"), (.lastState.hasFocus|tostring), (.lastState.wakeLock // "-"), (.cores|tostring), (.lastSeen // "-")]) | @tsv' | column -t -s $'\t' ;;
    queue) need_home; cmd_queue "$@" ;;
    wait) need_home; cmd_wait "$@" ;;
    report)
        need_home
        id="${1:?lab report needs a job id}"
        f="$home/jobs/$id/report.md"
        [[ -f "$f" ]] || { echo "lab: no report for $id yet ($f)" >&2; exit 1; }
        cat "$f" ;;
    jobs)
        need_home
        api /jobs | jq -r '
            ["id","state","builds","presses","tainted","failed","device","note"],
            (.jobs[] | [.id, .state, (.builds|join(" vs ")), "\(.presses.done)/\(.presses.total)", (.presses.tainted|tostring), (.presses.failed|tostring), (.boundDeviceName // .boundDevice // .device), (.note // "")]) | @tsv' | column -t -s $'\t' ;;
    cancel)
        need_home
        id="${1:?lab cancel needs a job id}"
        api "/jobs/$id" -X DELETE | jq -r '"lab: \(.id) \(.state)"' >&2 ;;
    curl)
        need_home
        p="${1:?lab curl needs a path}"; shift
        api "$p" "$@" ;;
    -h|--help|help|"") usage; [[ -n "$cmd" ]] || exit 2 ;;
    *) echo "lab: unknown command $cmd" >&2; usage >&2; exit 2 ;;
esac
