# The S3's IRAM hot set

~40 KB of flash functions — the engine's per-lane graph walk, the allocator,
the hottest shader builtins — linked into `.rwtext` (internal SRAM) instead of
`.text` (flash, through the 32 KB instruction cache). The decision and its
numbers are `docs/adr/2026-10-24-s3-hot-code-in-iram.md`.

| File | What | Written by |
|---|---|---|
| `profile.tsv` | per function: silicon samples, its size at capture, its demangled name, its linker pattern | `scripts/s3-hot-iram.py fold` |
| `../rwtext_hook.x` | the hook esp-hal INCLUDEs inside `.rwtext` | `just s3-hot-iram-gen` (`scripts/s3-hot-iram.py gen`) |

Nobody edits either by hand. The profile is a measurement; the hook is the
profile, greedy by samples per byte under 40,960 B, and the same profile
always writes the same hook.

## What keeps it current

`just s3-hot-iram-check` runs inside `just fw-esp32s3-size-check`, which CI's
S3 size job runs on every firmware PR. It reads the split image's `p2.elf`
and fails when:

- no hook function landed in IRAM (the hook is not being applied);
- the hot set, with its literal pools, outgrows the arena's reservation
  (`HOT_IRAM_BYTES` in `src/main.rs`), which would otherwise come out of the
  main stack;
- functions the hook names are gone from the image (renamed, inlined away)
  and carry more than 10 % of the hot set's samples.

A pattern globs what changes between two builds of the same code: the crate
disambiguator and the back-references of a v0 symbol, and an LTO `.llvm.<n>`
suffix. The first profile (taken on the spike's branch on 2026-10-23)
matched every one of its 191 functions on `main` a day later, so ordinary
changes do not trip it. What does:

| The check says | Do |
|---|---|
| a function was renamed (its row's name is gone, a new name holds the same code) | edit that row's name and pattern in `profile.tsv`, `just s3-hot-iram-gen` — a rename is not a new measurement |
| functions are gone, nothing renamed | `just s3-hot-iram-gen`: the hook backfills from the profile's next-best rows |
| the engine's shape has moved (a frame plan, a new hot path), or the stale share keeps creeping | take a new profile (below) |

Re-profile at least when the per-frame engine path is restructured: a hot
set from an old profile still works, it just covers less.

## Taking a profile (one FS3 sitting, ~15 minutes)

The sampler is core 1 reading core 0's program counter from the debug
assistant (`src/pc_sampler.rs`, feature `pc-sampler`): unbiased by
interrupt masking, and it costs core 0 nothing. Profile an image **without**
the hook, so every function competes on its flash cost:

```bash
ESP_HAL_CONFIG_USE_RWTEXT_LD_HOOK=false just build-fw-esp32s3 pc-sampler
cp target/xtensa-esp32s3-none-elf/release-esp32s3/fw-esp32s3 <somewhere>/pcs.elf   # the hourly prune wipes target/
board take fixture-s3 --as "<you>" --for "hot-set profile" --wait
# known-good release first (fs3-queue rules), then the sampler image, then per project:
lp-cli upload <project> serial:<port>
lp-cli link capture <port> --console pcs--<project>.txt --request reboot --seconds 70
# known-good release last, then board drop
scripts/s3-hot-iram.py fold <somewhere>/pcs.elf pcs--*.txt --note "<board, date, image, projects>"
just s3-hot-iram-gen
```

(`ESP_HAL_CONFIG_USE_RWTEXT_LD_HOOK=false` in the environment wins over
`.cargo/config.toml`'s `[env]`; `touch src/main.rs` if cargo does not
rebuild.) The first profile's projects were lanes × LEDs 8×256, 8×128, 2×512
and 6×248 of a noise pattern, and `projects/test/skirt-620`: several lane
counts, because the graph walk's cost is per lane. Keep that mix, or say in
the `--note` why not.

The `[pcs]` lines are dropped by whole lines when the link falls behind; the
survivors are an unbiased subsample. A capture yields ~8,000 samples, about
85 % of them in flash text.

## The split image

Hot functions ride in the core image's RAM segment, which the loader copies
in at boot. `lp-fw-split` does not root them (`Node::is_hot_ram_code`): an
engine function in IRAM stays engine by reachability, so it may call engine
code, and the core never calls it. Core and engine always come from one link
(the core's digest slot names its engine), so the engine code in the core's
RAM always matches the engine that calls it.
