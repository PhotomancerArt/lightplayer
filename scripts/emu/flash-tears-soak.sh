#!/usr/bin/env bash
# flash-tears-soak.sh [--batches N] [--date YYYY-MM-DD] [--keep-lease] [--dry-run]
#
# The tree-store M4 desk sitting in one command: power cuts on the SACRIFICIAL
# C6, recorded through the validation system. Each batch is one
# `lp-cli validate record flash-tears --config silicon:esp32c6`, which builds
# the `test_flash_tears` image, flashes it, and hands the port to
# `scripts/emu/flash-tears-cuts.py` for fifty cuts (the payload's
# `Capture::PowerCuts`). Each batch's transcript lands under
# `lp-emu/transcripts/esp32c6/flash-tears/`, a second one on the same day as
# `<stem>-r2`, and so on; nothing is ever overwritten or edited.
#
#   scripts/emu/flash-tears-soak.sh --batches 10        # the whole 500-cut sitting
#   scripts/emu/flash-tears-soak.sh --batches 1         # one foreground run of 50
#   scripts/emu/flash-tears-soak.sh --dry-run           # print the protocol, touch nothing
#
# Then read them: scripts/emu/flash-tears-analyze.py lp-emu/transcripts/esp32c6/flash-tears/silicon-*.txt
#
# It refuses unless the desk registry shows the board tagged `sacrificial`
# with the allocated MAC (M4 director ruling DD6) — the image destroys the
# filesystem on whatever it is flashed to. It leases the board as
# $BOARD_HOLDER (default "direct: tree-store M4"), resolves the port by MAC
# (never by a port-name pattern), cuts power only through `board
# power-cycle`, and drops the lease when it is done unless --keep-lease.
# It refuses a dirty tree under lp-fw/: the image's commit is the
# transcript's, stated, and a dirty build would make that a lie.
set -euo pipefail

SLUG="c6-expendable"
MAC="14:C1:9F:E6:54:90"
HOLDER="${BOARD_HOLDER:-direct: tree-store M4}"
batches=1
date="$(date +%F)"
keep_lease=""
dry_run=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --batches) batches="$2"; shift 2 ;;
        --date) date="$2"; shift 2 ;;
        --keep-lease) keep_lease=1; shift ;;
        --dry-run) dry_run=1; shift ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done

cd "$(git rev-parse --show-toplevel)"

if ! command -v board >/dev/null; then
    echo "REFUSED: \`board\` is not installed; there is no registry to say this board is sacrificial" >&2
    exit 3
fi
scripts/emu/flash-tears-cuts.py --check-board "$SLUG" --mac "$MAC"

if [[ -n "$(git status --porcelain -- lp-fw)" ]]; then
    echo "REFUSED: lp-fw/ has uncommitted changes; the transcript names the image's commit" >&2
    exit 3
fi
commit="$(git rev-parse --short=12 HEAD)"

if [[ -n "$dry_run" ]]; then
    BOARD_HOLDER="$HOLDER" cargo run -q -p lp-cli -- validate record flash-tears \
        --config silicon:esp32c6 --port "<resolved by MAC $MAC>" --date "$date" \
        --commit "$commit" --dry-run
    exit 0
fi

board take "$SLUG" --as "$HOLDER" --for "M4 flash-tears soak" --minutes 60 >/dev/null \
    || board renew "$SLUG" --as "$HOLDER" --minutes 60
if [[ -z "$keep_lease" ]]; then
    trap 'board drop "$SLUG" --as "$HOLDER"' EXIT
fi
port="$(BOARD_HOLDER="$HOLDER" cargo run -q -p lp-cli -- fwcheck port --mac "$MAC" | tail -1)"
echo "port $port, firmware $commit, $batches batch(es) of 50 cuts"

for ((b = 1; b <= batches; b++)); do
    board renew "$SLUG" --as "$HOLDER" --minutes 60 >/dev/null
    echo "=== batch $b of $batches ==="
    BOARD_HOLDER="$HOLDER" cargo run -q -p lp-cli -- validate record flash-tears \
        --config silicon:esp32c6 --port "$port" --date "$date" --commit "$commit"
    # The cut driver re-resolves the port after every cut; pick it up again
    # for the next flash.
    port="$(BOARD_HOLDER="$HOLDER" cargo run -q -p lp-cli -- fwcheck port --mac "$MAC" | tail -1)"
done
