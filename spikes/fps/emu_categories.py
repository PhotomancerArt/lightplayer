#!/usr/bin/env python3
"""SPIKE (fps spike): categorise emulated (t1) PC samples of the engine path.
Usage: emu_categories.py <a.pcs> <b.pcs> <elf> <rom.elf>
Prints per-category and per-function sample deltas a-b (a = more lanes)."""
import bisect, collections, glob, os, re, subprocess, sys
NM = glob.glob(os.path.expanduser("~/.rustup/toolchains/esp/xtensa-esp-elf/*/xtensa-esp-elf/bin/xtensa-esp32s3-elf-nm"))[0]
def syms(elf):
    r = []
    for line in subprocess.run([NM, "-C", "-n", elf], capture_output=True, text=True).stdout.splitlines():
        p = line.split(" ", 2)
        if len(p) == 3 and p[0] and p[1] in "tTwW": r.append((int(p[0], 16), p[2]))
    return sorted(r)
a, b, elf, rom = sys.argv[1:5]
t = syms(elf) + syms(rom); t.sort(); ad = [x for x, _ in t]
def name(pc):
    if 0x42800000 <= pc < 0x43000000: return "JIT"
    i = bisect.bisect_right(ad, pc) - 1
    return t[i][1] if i >= 0 and pc - ad[i] < 0x8000 else "?"
CATS = [("allocator", r"esp_alloc|linked_list_allocator|__rust_alloc|__rust_dealloc|RawVec|finish_grow|drop_in_place|::clone"),
        ("strings/paths/hash", r"SlotPath|slot_path|String|fmt::|Hash|hash|FnvHasher|::parse|str::|memcmp|compare"),
        ("resolver/query", r"resolver|resolve_|query_intern|QueryIntern|ResolveHost|EngineSession|collect_fragments|ResolverCache"),
        ("node tree/bindings", r"node_tree|NodeTree|binding|NodeEntry|bus_|consumers_for|VecMap|VecSet|lookup"),
        ("fixture per-lamp", r"stream_direct|DirectCoordFill|write_direct|encode_fixture|ControlRenderTarget|coordinates"),
        ("output/pipeline", r"display_pipeline|DisplayPipeline|provider|LcdBus|lcd_|ws281x"),
        ("node bodies", r"OutputNode|FixtureNode|ShaderNode|output_node|fixture_node|shader_node|Project>::tick|engine::engine"),
        ("memcpy/memset", r"^memcpy|^memset|^memmove"),
        ("shader", r"JIT|__lp|__lps|lpvm|LpvmShader|__divdi3|render_samples"),
        ("recovery", r"recovery|Recovery")]
def load(f):
    c = collections.Counter()
    for line in open(f):
        _, pc, n = line.split(); c[name(int(pc, 16))] += int(n)
    return c
A, B = load(a), load(b)
d = {k: A.get(k, 0) - B.get(k, 0) for k in set(A) | set(B)}
cat = collections.Counter()
for k, v in d.items():
    for cname, rx in CATS:
        if re.search(rx, k): cat[cname] += v; break
    else: cat["other"] += v
tot = sum(v for v in cat.values())
print("delta samples (256 instr each):", tot)
for k, v in cat.most_common(): print(f"{100*v/tot:6.1f}%  {k}")
print("--- top functions")
for k, v in sorted(d.items(), key=lambda kv: -kv[1])[:40]: print(f"{v:8d}  {k[:130]}")
