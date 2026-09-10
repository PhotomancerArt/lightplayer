#!/usr/bin/env bash
# ab.sh <slug> <grade> <timeout> <reps> -- <tag>:<bin>:<extra args> ...
# Alternating same-window A/B. Prints USER seconds per rep, then the best per
# tag with the run's `stopped after` line (the identity oracle).
set -u
cd "$(dirname "$0")"
slug="$1"; grade="$2"; to="$3"; reps="$4"; shift 4
[ "$1" = "--" ] && shift
configs=("$@")

elf="../emu-ref/8ffc4b325-$slug/fw-esp32c6"
: >"out/ab-$slug-$grade.log"

for rep in $(seq "$reps"); do
  for c in "${configs[@]}"; do
    tag="${c%%:*}"; rest="${c#*:}"; bin="${rest%%:*}"; extra="${rest#*:}"
    out="out/ab-$tag-$slug-$grade"
    # shellcheck disable=SC2086
    timeout 580 /usr/bin/time -p "./$bin" --elf "$elf" \
      --timeout "$to" --wall-timeout 570 --exit-on '[render-loop] === DONE ===' \
      --uart0 "file:$out.uart" --dump-frames "file:$out.jsonl" \
      --time-grade "$grade" $extra >"$out.out" 2>"$out.err"
    u=$(awk '/^user /{print $2}' "$out.err")
    w=$(awk '/^real /{print $2}' "$out.err")
    echo "$tag $u $w" >>"out/ab-$slug-$grade.log"
    printf '  rep%s %-6s user %ss wall %ss\n' "$rep" "$tag" "$u" "$w"
  done
done

echo "--- best user seconds ($slug $grade), load $(sysctl -n vm.loadavg | awk '{print $2}') ---"
for c in "${configs[@]}"; do
  tag="${c%%:*}"
  out="out/ab-$tag-$slug-$grade"
  b=$(awk -v t="$tag" '$1==t{if(m==""||$2<m)m=$2}END{print m}' "out/ab-$slug-$grade.log")
  st=$(grep -ao 'stopped after [0-9]* cycles ([0-9]* us emulated, [0-9]* instructions' "$out.err" | head -1)
  printf '%-6s best_user=%-7s %s\n' "$tag" "$b" "$st"
done
