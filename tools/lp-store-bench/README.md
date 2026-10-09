# lp-store-bench

The storage testbed: races on-device store candidates through simulated power
cuts on [`lp-nor-sim`](../../lp-emu/lp-nor-sim) and writes a JSONL
scoreboard. Host tooling for the spike
`lp2025/2026-10-07-1858-lpfs-fit-spike` (M6 testbed + M7 race); **nothing
here is linked into firmware**. Every number it prints is a *simulator*
number — not an emulator or silicon measurement.

## Pieces

- **Candidate** (`candidate.rs`): `Candidate` is the factory (`format`,
  `mount` — mount must never panic on any flash content); a mount yields a
  `CandidateStore` (`put`, `get`, `delete_prefix`, `list`, `commit`,
  `into_flash`, `flash_snapshot`, `report`). Object-safe, so drivers fan out
  over a list. Dials ride in `CandidateConfig.dials` as strings; on the
  command line a candidate is `name[@dial=v+dial=v]`, e.g.
  `t1@record_max=512+codec=stored` (`sectors=N` sets the partition). The
  littlefs candidates (`f1`, `f2`, `f3`) take `block_cycles=N` (littlefs's
  metadata-pair wear levelling; unset is −1, off, as the firmware ships —
  except `f3`, whose unset is 100; `f3@block_cycles=-1` turns it off).
- **Candidates** (`candidates/`): `mem` (the reference: whole store as one
  blob, ping-pong slots, CRC + sequence — correct under every cut) and
  `mem-broken` (its twin, erase-then-rewrite in place — fails), plus the
  real candidates as they land; `f3` (the control: littlefs, one deflated
  package per pattern, `/projects/<slot>/modules/<p>.pkg`) is in
  `littlefs_pattern_package.rs`.
- **T1** (`candidates/tree_store_candidate.rs`): `lp-tree-store` v1. A
  workload step is one store transaction (`begin` at its first write,
  `commit` at its end), so T1 is scored step-atomic. Dials: `record_max`
  (default 1024), `gc_policy` (`greedy` | `cost_benefit`), `reserve`,
  `txn_delta_max`, and `codec` = `stored` (every file written stored) |
  `host_deflate` (every file but the board's own `…/.lp/panel.json` arrives
  as the wire carries it after M6: `host_deflate_chunks` → ≤ 4 KiB logical
  chunks, `put_chunk_deflated` at offset 0 then the running size). The
  prototype's `deflate`, `deflate_dict`, `dict_size` and `json_tree` dials
  are gone with what they selected.
- **Workloads** (`workload.rs`): steps of puts and prefix deletes, each
  ending in `commit`, built from corpora at runtime, deterministic by seed.
  `push` (board files, then the corpus into `/projects/a/`), `repush` (the
  whole project again with 3 shaders ±50 B), `save` (20 steps of 1–3 docs),
  `panel` (100 rewrites of `/projects/a/.lp/panel.json`, ~450 B), `switch`
  (`a,b`: A replaced by B and back). Spec form `kind:corpus[@seed]`; a corpus
  is a directory under `--corpus` or `syn:<modules>:<shader_len>`
  (synthetic, what the unit tests use). JSON edits rewrite digits only (the
  shape stays canonical); shader edits grow or shrink a trailing comment.
- **Oracle** (`oracle.rs`): after a cut — power cycle, mount (`unmountable`
  if it fails), then **every path is its old or new value** for the
  interrupted step (absent counts; required for eligibility); whole-step
  atomicity (state == old or new) is scored as `non_atomic`; then the store
  must re-run the step to exactly `new`, take one more step, and survive a
  remount (`rerun_*`, `next_*`, `remount_*`). Panics are caught and scored
  (`panic`); a read watchdog (20 M reads per power cycle) scores loops.
- **Drivers**: `driver_exhaustive` (every cut point × tear model × seed of
  each swept step, parallel), `driver_double_cut` (sampled first cuts, then
  a cut at every/sampled op of the following mount, or of the re-run),
  `driver_random` (a seeded walk over push / re-push / save / panel /
  delete across three slots, a cut on 1 step in N, continuing from the
  recovered flash), `driver_measure` (fault-free: space, peak, write
  amplification, erase spread, mount cost, RAM; `min_sectors` binary-searches
  the smallest partition a workload completes in).
- **Cases fork, they don't replay**: a sweep runs the workload once
  fault-free, keeping the flash before each step (a clone is cheap); each
  case mounts that flash and cuts at `mount_ops + k`, where `mount_ops` is
  the mount's own op count (deterministic, so the cut lands in the step).

## Commands

```bash
cargo run --release -p lp-store-bench -- smoke --candidates mem,mem-broken
cargo run --release -p lp-store-bench -- sweep --candidates t1 --workloads 'push:c40;save:c13' --seeds 1,2
cargo run --release -p lp-store-bench -- double --candidates f2 --workloads save:c13
cargo run --release -p lp-store-bench -- random --candidates s1 --seeds 8 --steps 300
cargo run --release -p lp-store-bench -- measure --candidates f1,f2,s1,t1 --workloads 'push:c40;save:c40' --min-sectors
cargo run --release -p lp-store-bench -- endurance --candidates 'f1,f1@block_cycles=100' --corpus-name c20 --sectors 176 --pushes 0
cargo run --release -p lp-store-bench -- replay failure.json      # one scoreboard line
```

### Overnight

```bash
nohup nice -n 10 target/release/lp-store-bench overnight --until 09:00 --threads 8 \
  --out <dir> --corpus <dir> > <dir>/overnight.log 2>&1 &
target/release/lp-store-bench report --out <dir>      # re-render report.md at any time
target/release/lp-store-bench overnight --quick --until +3m --candidates mem --out /tmp/x   # runner smoke
```

`overnight` works a priority list against its deadline (checked between
units and between the steps of a sweep), writing every result as it lands:

1. fault-free measures — every candidate × c13/c20/c40/c40reuse/c40-min-z ×
   push/repush/save/panel (+ switch), with min-partition searches for push
   and save;
2. exhaustive single cuts — push/repush/save/panel on c40 (c20 for `f1`,
   which cannot hold c40) and switch c13↔c40reuse, every cut point, every
   tear model, 2 seeds; units interleave candidates;
3. double cuts — the same workloads, the first 4 focus steps;
4. the T1 dial sweep — `record_max` × `gc_policy` × `reserve` × codec (`stored` | `host_deflate`) ×
   partition {96, 128, 176}: fault-free c40 measures plus a reduced cut
   sweep each;
5. endurance — 30 simulated days (1 re-push, 10 saves, 1440 panel writes a
   day);
6. fill to full — c20 copies until `NoSpace` then saves; the largest
   c40-style project (edited module copies) that pushes and takes 5 saves;
7. random walks — 16 seeds × 300 steps per candidate.

Then rounds repeat with new seeds (odd rounds: cut sweeps, double cuts and
walks; even rounds: walks) until the deadline. The report's "What did not
run" lists the units still queued at the end. `report` renders `report.md`
(headline, cut totals with a replay command per failure, measures, fill, the
T1 dial Pareto front) and `summary.json`.

`--corpus` defaults to the spike's `measurements/corpus`; `--out` to
`target/lp-store-bench/<cmd>`; `--threads` 8; `--sectors` 128.

## Scoreboard (`<out>/scoreboard.jsonl`)

One JSON object per line, appended and flushed per line; `"type"` says
which:

| type | fields |
|---|---|
| `measure` | `candidate`, `config {sectors, dials}`, `workload {kind, corpus, seed}`, `ok`, `error`, `failed_step`, `logical_bytes`, `program_bytes`, `write_amp`, `erases_{total,min,median,max}`, `sectors_nonblank_{end,peak}` (the flash's view), `used_sectors_{end,max}` (the store's own), `mount_{read_bytes,read_calls,ops}`, `report {ram_bytes, used_sectors, step_atomic, extra}`, `violations_0_to_1`, `live_logical_bytes` |
| `endurance` | `days`, `corpus`, `spec` (the `endurance` command; the overnight run's has no `spec`), `result` (a `measure` object) |
| `min_sectors` | `candidate`, `config`, `workload`, `min_sectors` (null = does not fit in 512) |
| `sweep_summary` | `driver` (`exhaustive` / `double_cut`), `candidate`, `config`, `workload`, `tear`, `cases`, `landed`, `failures`, `non_atomic`, `kinds {failure kind: count}`, `steps_swept`, `steps_skipped`, `max_cuts_per_step`, `error` |
| `random_summary` | `candidate`, `config`, `seed`, `steps_run`, `steps_no_space`, `cuts`, `failures`, `non_atomic`, `kinds`, `first_failure`, `error` |
| `failure` | `driver`, `failure {kind, detail}`, `reproducer` (`{"case": {candidate, config, workload, step, cut_after, tear, seed, second}}` or `{"random": {…, stop_at_cut}}`) — feed the line to `replay` |

At most 20 full failure records are written per (sweep, tear) and 5 per
random walk; the summaries count all of them.
