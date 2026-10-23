#!/usr/bin/env python3
"""SPIKE: fold `LP_EMU_PC_SAMPLES` output into functions and buckets.
Usage: symbolize.py <samples> <fw.elf> <rom.elf> [core]"""
import bisect, subprocess, sys, collections, os
NM = "/Users/yona/.rustup/toolchains/esp/xtensa-esp-elf/esp-14.2.0_20240906/xtensa-esp-elf/bin/xtensa-esp32s3-elf-nm"
def syms(elf):
    out = subprocess.run([NM, "-C", "-n", elf], capture_output=True, text=True).stdout
    rows = []
    for line in out.splitlines():
        parts = line.split(" ", 2)
        if len(parts) == 3 and parts[0] and parts[1] in "tTwW":
            rows.append((int(parts[0], 16), parts[2]))
    rows.sort()
    return rows
samples, fw, rom = sys.argv[1:4]
want = int(sys.argv[4]) if len(sys.argv) > 4 else None
table = syms(fw) + syms(rom)
table.sort()
addrs = [a for a, _ in table]
funcs = collections.Counter(); buckets = collections.Counter(); total = 0
def bucket(pc, name):
    if 0x40000000 <= pc < 0x40060000: return "ROM"
    if 0x42800000 <= pc < 0x43000000: return "JIT (PSRAM I-bus)"
    if 0x40378000 <= pc < 0x403e0000 and name is None: return "JIT? (SRAM)"
    n = name or "?"
    for key, b in [("lps_builtins", "shader builtins"), ("lpvm_native", "lpvm-native"),
                   ("lpc_engine", "lpc-engine"), ("lp_gfx", "lp-gfx"), ("lp_ws281x", "ws281x encode"),
                   ("esp_alloc", "allocator"), ("lpc_hardware", "lpc-hardware"),
                   ("fw_esp32s3::core1", "core1 idle"), ("esp_rtos", "esp-rtos"),
                   ("esp_radio", "radio"), ("trouble", "radio"), ("lps_q32", "q32 math"),
                   ("compiler_builtins", "compiler-builtins"), ("core::", "core"), ("alloc::", "alloc")]:
        if key in n: return b
    return "other"
for line in open(samples):
    core, pc, n = line.split(); core = int(core); pc = int(pc, 16); n = int(n)
    if want is not None and core != want: continue
    i = bisect.bisect_right(addrs, pc) - 1
    name = table[i][1] if i >= 0 and pc - addrs[i] < 0x4000 else None
    b = bucket(pc, name)
    if b.startswith("JIT"): name = b
    funcs[name or f"?{pc:#x}"] += n; buckets[b] += n; total += n
print(f"total samples {total}")
for b, n in buckets.most_common(): print(f"{100*n/total:6.1f}%  {b}")
print("--- top functions")
for f, n in funcs.most_common(30): print(f"{100*n/total:6.1f}%  {f[:140]}")
