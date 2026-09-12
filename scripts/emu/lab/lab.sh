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

usage() { sed -n '2,25p' "$0"; }

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

cmd="${1:-}"; [[ $# -gt 0 ]] && shift
case "$cmd" in
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
