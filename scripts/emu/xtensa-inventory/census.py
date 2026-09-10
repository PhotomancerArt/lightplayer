#!/usr/bin/env python3
"""Exact decoder-coverage census of an Xtensa artefact against `lp-xt-inst`.

Why this exists
---------------
`lp-xt/lp-xt-inst/src/bin/objdiff.rs` is the decoder-coverage oracle: it
disassembles an ELF with `lp-xt-inst`'s own `decode()` and diffs every
instruction against GNU `objdump`, so its numbers are `decode()`-exact rather
than a mnemonic-name-table proxy. It has one limitation for an inventory: it
looks only at the section literally named `.text`.

Real artefacts carry executable code in several sections — the v3 firmware
image has `.text`, `.rwtext` and `.vectors`; the classic mask ROM has
`.text`, `.bt_text`, two secure-boot patch sections and eleven vector
sections; the IDF second-stage bootloader is not an ELF at all.

This script closes that gap **without touching `lp-xt-inst`** (M1 owns that
crate). For each CODE section it uses `objcopy` to lift the section's bytes
into a one-section ELF whose single section is named `.text` and carries the
original VMA, then runs `objdiff` on that. The disassembly objdump produces is
identical either way — same bytes, same load address — so the per-section
numbers sum to an exact whole-artefact figure.

Licence note (AGENTS.md): binutils is used here as a *tool whose output is
fact*. No binutils source, table, or logic is read or adapted.

Usage
-----
    scripts/emu/xtensa-inventory/census.py <artefact> [--out DIR]
        [--objdump PATH] [--objdiff PATH] [--base ADDR] [--json FILE]

`<artefact>` is an Xtensa ELF, or a raw binary when `--base` gives its load
address (that is the bootloader case).

Output: a per-section table, the aggregate totals, the ranked unsupported
mnemonics, and the special/user register census, on stdout; optionally the
same as JSON.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from collections import Counter
from pathlib import Path

DEFAULT_TOOLCHAIN = Path(
    os.path.expanduser(
        "~/.rustup/toolchains/esp/xtensa-esp-elf/esp-14.2.0_20240906/xtensa-esp-elf/bin"
    )
)

# objdump's section header table, two lines per section:
#   " 10 .text         001c8231  400d0020  400d0020  00051020  2**2"
#   "                 CONTENTS, ALLOC, LOAD, READONLY, CODE"
_SEC_RE = re.compile(
    r"^\s*\d+\s+(\S+)\s+([0-9a-f]+)\s+([0-9a-f]+)\s+([0-9a-f]+)\s+([0-9a-f]+)\s+2\*\*"
)

# objdiff's own report lines.
_COUNT_RE = re.compile(r"^\s*(instructions \(objdump\)|supported \(decoded\)|matched|MISMATCHED|unsupported|data-directive bytes):\s+(\d+)")
_TALLY_RE = re.compile(r"^\s{2,}(\d+)\s{2}(\S.*)$")

SR_RE = re.compile(r"^(rsr|wsr|xsr|rur|wur)\.(\S+)$")


def run(cmd: list[str]) -> str:
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode not in (0, 1):  # objdiff exits 1 when mismatches exist
        sys.exit(f"census: command failed ({proc.returncode}): {' '.join(cmd)}\n{proc.stderr}")
    return proc.stdout


def code_sections(objdump: Path, elf: Path) -> list[tuple[str, int, int]]:
    """Every section flagged CODE, as `(name, size, vma)`, size > 0."""
    lines = run([str(objdump), "-h", str(elf)]).splitlines()
    out: list[tuple[str, int, int]] = []
    for i, line in enumerate(lines):
        m = _SEC_RE.match(line)
        if not m:
            continue
        flags = lines[i + 1] if i + 1 < len(lines) else ""
        if "CODE" not in flags:
            continue
        name, size, vma = m.group(1), int(m.group(2), 16), int(m.group(3), 16)
        if size:
            out.append((name, size, vma))
    return out


def lift_section(objcopy: Path, elf: Path, name: str, vma: int, workdir: Path) -> Path:
    """Lift one section into a one-section ELF whose section is `.text` at `vma`."""
    raw = workdir / f"{name.strip('.').replace('.', '_')}.bin"
    out = workdir / f"{name.strip('.').replace('.', '_')}.elf"
    run([str(objcopy), "-O", "binary", f"--only-section={name}", str(elf), str(raw)])
    run(
        [
            str(objcopy),
            "-I", "binary",
            "-O", "elf32-xtensa-le",
            "-B", "xtensa",
            "--rename-section", ".data=.text,alloc,load,readonly,code",
            "--change-section-address", f".data={vma:#x}",
            str(raw),
            str(out),
        ]
    )
    return out


def wrap_raw(objcopy: Path, blob: Path, vma: int, workdir: Path) -> Path:
    out = workdir / (blob.stem + ".elf")
    run(
        [
            str(objcopy),
            "-I", "binary",
            "-O", "elf32-xtensa-le",
            "-B", "xtensa",
            "--rename-section", ".data=.text,alloc,load,readonly,code",
            "--change-section-address", f".data={vma:#x}",
            str(blob),
            str(out),
        ]
    )
    return out


def parse_objdiff(text: str) -> dict:
    """Pull the counters and the two mnemonic tallies out of an objdiff report."""
    counts: dict[str, int] = {}
    supported: Counter = Counter()
    unsupported: Counter = Counter()
    bucket = None
    for line in text.splitlines():
        m = _COUNT_RE.match(line)
        if m:
            counts[m.group(1)] = int(m.group(2))
            continue
        if line.startswith("--- supported opcodes"):
            bucket = supported
            continue
        if line.startswith("--- UNSUPPORTED allowlist"):
            bucket = unsupported
            continue
        if line.startswith("--- MISMATCHES"):
            bucket = None
            continue
        if bucket is not None:
            t = _TALLY_RE.match(line)
            if t:
                bucket[t.group(2).strip()] += int(t.group(1))
    return {
        "insns": counts.get("instructions (objdump)", 0),
        "decoded": counts.get("supported (decoded)", 0),
        "matched": counts.get("matched", 0),
        "mismatched": counts.get("MISMATCHED", 0),
        "unsupported_n": counts.get("unsupported", 0),
        "data_bytes": counts.get("data-directive bytes", 0),
        "supported": supported,
        "unsupported": unsupported,
        "raw": text,
    }


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("artefact", type=Path)
    ap.add_argument("--base", help="load address; makes <artefact> a raw binary")
    ap.add_argument("--objdump", type=Path, default=DEFAULT_TOOLCHAIN / "xtensa-esp32-elf-objdump")
    ap.add_argument("--objcopy", type=Path, default=DEFAULT_TOOLCHAIN / "xtensa-esp32-elf-objcopy")
    ap.add_argument("--objdiff", type=Path, default=Path("target/release/objdiff"))
    ap.add_argument("--out", type=Path, help="directory to keep the per-section objdiff reports")
    ap.add_argument("--json", type=Path)
    ap.add_argument("--skip", action="append", default=[], help="section name to exclude")
    args = ap.parse_args()

    if not args.objdiff.exists():
        sys.exit(
            f"census: {args.objdiff} not found — build it first:\n"
            "  cargo build -p lp-xt-inst --features objdiff --bin objdiff --release"
        )

    env_objdump = str(args.objdump)
    tmp = tempfile.TemporaryDirectory(prefix="xt-census-")
    workdir = Path(tmp.name)
    outdir = args.out
    if outdir:
        outdir.mkdir(parents=True, exist_ok=True)

    if args.base:
        base = int(args.base, 0)
        secs = [(args.artefact.name, args.artefact.stat().st_size, base)]
        elves = {args.artefact.name: wrap_raw(args.objcopy, args.artefact, base, workdir)}
    else:
        secs = [s for s in code_sections(args.objdump, args.artefact) if s[0] not in args.skip]
        elves = {
            name: lift_section(args.objcopy, args.artefact, name, vma, workdir)
            for (name, _size, vma) in secs
        }

    rows = []
    tot_sup: Counter = Counter()
    tot_uns: Counter = Counter()
    totals = Counter()
    for name, size, vma in secs:
        env = dict(os.environ, XT_OBJDUMP=env_objdump)
        proc = subprocess.run(
            [str(args.objdiff), str(elves[name])], capture_output=True, text=True, env=env
        )
        rep = parse_objdiff(proc.stdout)
        if outdir:
            (outdir / f"{name.strip('.').replace('.', '_')}.objdiff.txt").write_text(proc.stdout)
        rows.append((name, size, vma, rep))
        tot_sup.update(rep["supported"])
        tot_uns.update(rep["unsupported"])
        for k in ("insns", "decoded", "matched", "mismatched", "unsupported_n", "data_bytes"):
            totals[k] += rep[k]

    def pct(n: int, d: int) -> str:
        return f"{100.0 * n / d:.2f}%" if d else "n/a"

    print(f"=== xtensa-inventory census: {args.artefact} ===")
    print(f"objdump: {args.objdump}")
    print()
    print(f"{'section':<28} {'bytes':>9} {'vma':>10} {'insns':>9} {'decoded':>9} {'cover':>8} {'mism':>5} {'unsup':>7} {'data':>7}")
    for name, size, vma, r in rows:
        print(
            f"{name:<28} {size:>9} {vma:>#10x} {r['insns']:>9} {r['decoded']:>9} "
            f"{pct(r['decoded'], r['insns']):>8} {r['mismatched']:>5} {r['unsupported_n']:>7} {r['data_bytes']:>7}"
        )
    print(
        f"{'TOTAL':<28} {sum(s for _, s, _ in secs):>9} {'':>10} {totals['insns']:>9} "
        f"{totals['decoded']:>9} {pct(totals['decoded'], totals['insns']):>8} "
        f"{totals['mismatched']:>5} {totals['unsupported_n']:>7} {totals['data_bytes']:>7}"
    )
    print()
    print(f"exact coverage: {totals['decoded']}/{totals['insns']} = {pct(totals['decoded'], totals['insns'])} decoded; "
          f"{totals['matched']}/{totals['insns']} = {pct(totals['matched'], totals['insns'])} decoded AND matching objdump")
    print()

    print(f"--- unsupported mnemonics ({len(tot_uns)} kinds, {sum(tot_uns.values())} sites) ---")
    for mnem, n in tot_uns.most_common():
        print(f"  {n:>7}  {mnem}")
    print()

    srs: Counter = Counter()
    for src in (tot_sup, tot_uns):
        for mnem, n in src.items():
            m = SR_RE.match(mnem.split(" ")[0])
            if m:
                srs[(m.group(2), m.group(1))] += n
    print(f"--- special / user registers ({len({r for r, _ in srs})} registers, {sum(srs.values())} sites) ---")
    for (reg, op), n in sorted(srs.items(), key=lambda kv: (-kv[1], kv[0])):
        print(f"  {n:>7}  {op}.{reg}")
    print()

    print(f"--- supported mnemonics ({len(tot_sup)} kinds, {sum(tot_sup.values())} sites) ---")
    for mnem, n in tot_sup.most_common():
        print(f"  {n:>7}  {mnem}")

    if args.json:
        args.json.write_text(
            json.dumps(
                {
                    "artefact": str(args.artefact),
                    "sections": [
                        {
                            "name": n, "bytes": s, "vma": v,
                            "insns": r["insns"], "decoded": r["decoded"],
                            "matched": r["matched"], "mismatched": r["mismatched"],
                            "unsupported": r["unsupported_n"], "data_bytes": r["data_bytes"],
                        }
                        for n, s, v, r in rows
                    ],
                    "totals": dict(totals),
                    "unsupported": dict(tot_uns),
                    "supported": dict(tot_sup),
                    "special_registers": {f"{op}.{reg}": n for (reg, op), n in srs.items()},
                },
                indent=2,
            )
        )
    tmp.cleanup()


if __name__ == "__main__":
    main()
