#!/bin/bash
# Native run: $1 = binary tag, $2 = image slug/dir, $3 = grade. Prints user s +
# the `stopped after` line + the uart sha.
cd /Users/yona/dev/photomancer/lp2025/.claude/worktrees/agent-a1676bc71e062fc30
BIN="target/p1b/$1"
case "$2" in
  rb) ELF=target/emu-ref/8ffc4b325-render-basic/fw-esp32c6 ;;
  rk) ELF=target/emu-ref/8ffc4b325-render-rocaille/fw-esp32c6 ;;
  hn) ELF=target/emu-ref/d6cfaa205-harness/fw-esp32c6 ;;
  bi) ELF=target/emu-ref/d6cfaa205-boot-idle-memfs/fw-esp32c6 ;;
esac
U="/tmp/p1b-$1-$2-$3.uart"
S=$( { /usr/bin/time -p "$BIN" --elf "$ELF" --timeout 8s --exit-on '[render-loop] === DONE ===' \
      --time-grade "$3" --uart0 "file:$U" ${EXTRA:-} > /tmp/p1b-out.txt; } 2>&1 )
USER=$(echo "$S" | awk '/^user/{print $2}')
STOP=$(grep -h "stopped after" /tmp/p1b-out.txt | head -1)
SHA=$(shasum -a 256 "$U" | cut -c1-12)
echo "$1 $2 $3 user=$USER uart=$SHA | $STOP"
