#!/usr/bin/env python3
"""OTA split-link spike, S1: how would one firmware ELF split into core + engine?

Inputs: an ELF linked with `--emit-relocs`, and lld's `-Map` file for it.

Nodes are the linker's input sections (the unit a linker script can place),
read from the map. Edges are the emitted relocations: source = the input
section holding r_offset, target = the input section holding S + A.

Core = everything reachable from the core roots without entering a function
the cut rules call "engine". Everything else is the engine. Code that lives
in RAM (.trap, .rwtext*) is always core: the bootloader loads it.

Spike tooling — not product code.
"""

import argparse
import bisect
import collections
import json
import re
import struct
import subprocess
import sys

# ---- ELF32 little-endian, just what we need ---------------------------------

SKIP_RELOCS = {
    0,  # NONE
    24, 25,  # PCREL_LO12_I/S: symbol is the paired auipc label, not a target
    33, 34, 35, 36, 37, 38, 39, 40,  # ADD/SUB (label differences)
    43,  # ALIGN
    51,  # RELAX
    52, 53, 54, 55, 56,  # SUB6/SET6/SET8/SET16/SET32
    57,  # 32_PCREL (difference-style)
}


def read_elf(path):
    data = open(path, "rb").read()
    assert data[:4] == b"\x7fELF" and data[4] == 1, "ELF32 only"
    (e_shoff,) = struct.unpack_from("<I", data, 0x20)
    e_shentsize, e_shnum, e_shstrndx = struct.unpack_from("<HHH", data, 0x2E)
    shdrs = []
    for i in range(e_shnum):
        off = e_shoff + i * e_shentsize
        f = struct.unpack_from("<IIIIIIIIII", data, off)
        shdrs.append(dict(name=f[0], type=f[1], flags=f[2], addr=f[3], offset=f[4],
                          size=f[5], link=f[6], info=f[7], entsize=f[9]))
    strtab = shdrs[e_shstrndx]

    def sname(o):
        s = strtab["offset"] + o
        return data[s:data.index(b"\0", s)].decode()

    for s in shdrs:
        s["sname"] = sname(s["name"])
    symtab = next(s for s in shdrs if s["type"] == 2)
    symstr = shdrs[symtab["link"]]
    syms = []
    for i in range(symtab["size"] // 16):
        n, v, sz, info, other, shndx = struct.unpack_from("<IIIBBH", data, symtab["offset"] + i * 16)
        s = symstr["offset"] + n
        syms.append((data[s:data.index(b"\0", s)].decode(errors="replace"), v, sz, info & 0xF, shndx))
    relas = []
    for s in shdrs:
        if s["type"] != 4:  # SHT_RELA
            continue
        tgt = shdrs[s["info"]]
        if not (tgt["flags"] & 2):  # SHF_ALLOC
            continue
        for i in range(s["size"] // 12):
            r_off, r_info, r_add = struct.unpack_from("<IIi", data, s["offset"] + i * 12)
            rtype, rsym = r_info & 0xFF, r_info >> 8
            if rtype in SKIP_RELOCS:
                continue
            relas.append((r_off, rtype, syms[rsym][1] + r_add, syms[rsym][0]))
    e_entry = struct.unpack_from("<I", data, 0x18)[0]
    return shdrs, syms, relas, e_entry


# ---- lld map: input sections --------------------------------------------------

MAP_LINE = re.compile(r"^\s*([0-9a-f]+)\s+([0-9a-f]+)\s+([0-9a-f]+)\s+(\d+)\s(\s*)(.*)$")


def read_map(path):
    """Yield (vma, size, out_section, input_desc) for every input section."""
    out = None
    nodes = []
    for line in open(path, errors="replace"):
        m = MAP_LINE.match(line)
        if not m:
            continue
        vma, lma, size, align, indent, rest = m.groups()
        vma, size = int(vma, 16), int(size, 16)
        depth = len(indent)
        if depth == 0:
            out = rest.strip()
        elif ":(" in rest and depth <= 8:
            nodes.append(dict(vma=vma, size=size, out=out, desc=rest.strip()))
    return nodes


# ---- naming -----------------------------------------------------------------

def demangled_symbols(elf):
    """address -> list of (size, demangled name), from rust-nm."""
    out = subprocess.run(["rust-nm", "--demangle", "--print-size", "--defined-only", elf],
                         capture_output=True, text=True).stdout
    byaddr = collections.defaultdict(list)
    for line in out.splitlines():
        p = line.split(" ", 3)
        if len(p) == 4:
            byaddr[int(p[0], 16)].append((int(p[1], 16), p[3]))
        elif len(p) == 3:
            byaddr[int(p[0], 16)].append((0, p[2]))
    return byaddr


CRATE_RE = re.compile(r"^<*(?:&(?:mut )?)?(?:dyn )?([a-z_][a-z0-9_]*)::")


def crate_of(name):
    m = CRATE_RE.match(name)
    return m.group(1) if m else None


def is_rust_object(obj):
    return ".rcgu.o" in obj or "lto.tmp" in obj or obj.endswith(".rlib")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("elf")
    ap.add_argument("map")
    ap.add_argument("--engine-crates", default="", help="comma list: edges into these crates' sections are cut")
    ap.add_argument("--engine-syms", default="", help="comma list of regexes on demangled names to cut")
    ap.add_argument("--keep-crates", default="", help="crates never cut even if matched")
    ap.add_argument("--min-door-size", type=int, default=32,
                    help="smaller nodes are never doors: LLVM merges identical tiny fns (e.g. every "
                         "no-op Debug::fmt under fmt-debug=none) under one arbitrary crate's name")
    ap.add_argument("--roots", default="", help="comma list of regexes: extra core roots (what a restructured core main would call)")
    ap.add_argument("--why", default="", help="regex on node name: print the root->node path for matches in core")
    ap.add_argument("--emit-ld", help="write a linker script placing every engine input section at --engine-base")
    ap.add_argument("--engine-base", default="0x42400000")
    ap.add_argument("--verify-engine-base", help="pass-2 check: no core node may live at/after this vaddr in flash")
    ap.add_argument("--json", help="write a json summary here")
    ap.add_argument("--top", type=int, default=25)
    args = ap.parse_args()

    shdrs, syms, relas, entry = read_elf(args.elf)
    nodes = read_map(args.map)
    flash_outs = {".text", ".rodata", ".text_gap", ".flash.appdesc", ".engine_text", ".engine_rodata"}
    ram_code_outs = {".trap", ".rwtext", ".rwtext.wifi"}
    ram_data_outs = {".data", ".data.wifi", ".bss", ".noinit", ".rtc_fast.text", ".rtc_fast.data",
                     ".rtc_fast.bss", ".rtc_fast.persistent", ".stack", ".dram2_uninit"}
    nodes = [n for n in nodes if n["size"] > 0 and (n["out"] in flash_outs | ram_code_outs | ram_data_outs)]
    nodes.sort(key=lambda n: n["vma"])
    starts = [n["vma"] for n in nodes]

    def node_at(addr):
        i = bisect.bisect_right(starts, addr) - 1
        if i >= 0 and nodes[i]["vma"] <= addr < nodes[i]["vma"] + nodes[i]["size"]:
            return i
        return None

    # Name each node by its largest symbol, and give it a crate.
    byaddr = demangled_symbols(args.elf)
    sorted_addrs = sorted(byaddr)
    for n in nodes:
        # The largest symbol at or inside the section names it.
        j = bisect.bisect_left(sorted_addrs, n["vma"])
        cands = byaddr[sorted_addrs[j]] if j < len(sorted_addrs) and sorted_addrs[j] < n["vma"] + n["size"] else []
        cands = sorted(cands, key=lambda c: -c[0])
        obj, _, sec = n["desc"].partition(":(")
        n["name"] = cands[0][1] if cands else sec.rstrip(")")
        if is_rust_object(obj):
            n["crate"] = crate_of(n["name"]) or "(rust-anon)"
        else:
            n["crate"] = "(C:" + obj.split("/")[-1][:48] + ")"

    edges = collections.defaultdict(set)
    unresolved = 0
    for r_off, rtype, target, symname in relas:
        s, t = node_at(r_off), node_at(target)
        if s is None or t is None:
            unresolved += 1
            continue
        if s != t:
            edges[s].add(t)

    engine_crates = {c for c in args.engine_crates.split(",") if c}
    keep = {c for c in args.keep_crates.split(",") if c}
    engine_res = [re.compile(r) for r in args.engine_syms.split(",") if r]

    def is_engine_door(i):
        n = nodes[i]
        if n["out"] in ram_code_outs or n["crate"] in keep or n["size"] < args.min_door_size:
            return False
        if n["crate"] in engine_crates:
            return True
        return any(r.search(n["name"]) for r in engine_res)

    roots = [i for i, n in enumerate(nodes) if n["out"] in ram_code_outs or n["out"] == ".flash.appdesc"]
    e = node_at(entry)
    if e is not None:
        roots.append(e)
    root_res = [re.compile(r) for r in args.roots.split(",") if r]
    extra = [i for i, n in enumerate(nodes) if any(r.search(n["name"]) for r in root_res)]
    print(f"extra roots: {len(extra)}", file=sys.stderr)
    roots += extra

    core = set()
    parent = {}
    cut_edges = collections.Counter()  # (src crate, dst name) -> count
    door_targets = set()
    stack = list(roots)
    while stack:
        i = stack.pop()
        if i in core:
            continue
        core.add(i)
        for t in edges.get(i, ()):
            if t in core:
                continue
            if is_engine_door(t):
                cut_edges[(nodes[i]["name"][:90], nodes[t]["name"][:90])] += 1
                door_targets.add(t)
                continue
            parent.setdefault(t, i)
            stack.append(t)

    if args.why:
        why = re.compile(args.why)
        shown = 0
        for i in sorted(core, key=lambda i: -nodes[i]["size"]):
            if why.search(nodes[i]["name"]) and shown < 8:
                shown += 1
                print(f"\n-- why in core: {nodes[i]['name'][:110]} ({nodes[i]['size']} B)")
                j, hops = i, 0
                while j in parent and hops < 40:
                    j = parent[j]
                    hops += 1
                    print(f"   <- [{nodes[j]['crate']}] {nodes[j]['name'][:110]}")

    def size_of(idx, outs):
        return sum(nodes[i]["size"] for i in idx if nodes[i]["out"] in outs)

    all_idx = set(range(len(nodes)))
    engine = all_idx - core
    flash = flash_outs - {".text_gap"}
    summary = dict(
        nodes=len(nodes), relocs=len(relas), unresolved_relocs=unresolved,
        core_flash=size_of(core, flash), engine_flash=size_of(engine, flash),
        ram_code=size_of(all_idx, ram_code_outs),
        engine_ram_data_unreached=size_of(engine, ram_data_outs),
        door_targets=len(door_targets), cut_edges=len(cut_edges),
    )
    print(json.dumps(summary, indent=1))

    def by_crate(idx):
        c = collections.Counter()
        for i in idx:
            if nodes[i]["out"] in flash:
                c[nodes[i]["crate"]] += nodes[i]["size"]
        return c

    print("\n== core flash by crate ==")
    for k, v in by_crate(core).most_common(args.top):
        print(f"{v:9d} {k}")
    print("\n== engine flash by crate ==")
    for k, v in by_crate(engine).most_common(args.top):
        print(f"{v:9d} {k}")
    print(f"\n== cut edges (core -> engine door), {len(cut_edges)} distinct ==")
    for (s, t), c in sorted(cut_edges.items())[: args.top * 4]:
        print(f"  {s}\n      -> {t}")
    if args.emit_ld:
        emit_ld(args, nodes, engine, flash)
    if args.verify_engine_base:
        base = int(args.verify_engine_base, 16)
        bad = [i for i in core if nodes[i]["out"] in flash and nodes[i]["vma"] >= base]
        eng_low = [i for i in engine if nodes[i]["out"] in flash and nodes[i]["vma"] < base]
        print(f"\n== verify: core nodes in engine region: {len(bad)}; engine nodes left in core region: {len(eng_low)} "
              f"({size_of(eng_low, flash)} B)")
        for i in bad[:20]:
            print(f"  CORE IN ENGINE REGION: {nodes[i]['name'][:120]}")
    if args.json:
        json.dump(dict(summary=summary,
                       core_by_crate=by_crate(core).most_common(),
                       engine_by_crate=by_crate(engine).most_common(),
                       cut_edges=[[s, t] for (s, t) in cut_edges]),
                  open(args.json, "w"), indent=1)


def emit_ld(args, nodes, engine, flash):
    """Engine input sections, by name, into two output sections at a fixed vaddr.

    Only sections from the Rust object are listed: a C archive member that only
    the engine reaches stays in core, which is harmless (core never calls it)
    and keeps the patterns simple.
    """
    text, ro = [], []
    for i in sorted(engine, key=lambda i: nodes[i]["vma"]):
        n = nodes[i]
        if n["out"] not in flash:
            continue
        obj, _, sec = n["desc"].partition(":(")
        if not is_rust_object(obj):
            continue
        sec = sec[:-1] if sec.endswith(")") else sec
        if sec == ".engine_header":
            continue
        # Section names are not unique across objects: compiler_builtins (never
        # LTO'd) carries its own OUTLINED_FUNCTION_N, so match the object too.
        # The object's hash changes with the link args, so glob it; archive
        # members use lld's `archive:member` spelling.
        base = re.sub(r"-[0-9a-f]{16}\b", "-*", obj.split("/")[-1])
        if "(" in base:
            archive, member = base[:-1].split("(", 1)
            base = f"{archive}:{member}"
        pat = f"*{base}({sec})"
        (text if n["out"] == ".text" else ro).append(pat)
    with open(args.emit_ld, "w") as f:
        f.write("/* OTA split-link spike: generated by scripts/ota-spike/split_reach.py. */\n")
        f.write("/* Engine region: mapped by the core at runtime, never by the bootloader. */\n")
        f.write(f"MEMORY {{\n  ENGINE : ORIGIN = {args.engine_base}, LENGTH = 0x400000\n}}\n")
        f.write("SECTIONS {\n")
        f.write("  .engine_rodata : ALIGN(4) {\n    KEEP(*(.engine_header))\n")
        for sct in ro:
            f.write(f"    {sct}\n")
        f.write("  } > ENGINE\n  .engine_text : ALIGN(4) {\n")
        for sct in text:
            f.write(f"    {sct}\n")
        f.write("  } > ENGINE\n}\nINSERT BEFORE .rodata;\n")
    print(f"emitted {args.emit_ld}: {len(ro)} rodata + {len(text)} text input sections", file=sys.stderr)


if __name__ == "__main__":
    main()
