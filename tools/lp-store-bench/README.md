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
cargo run --release -p lp-store-bench -- sweep --candidates t1 --workloads save:c40 --tears calibrated   # CX1's measured tears
cargo run --release -p lp-store-bench -- smoke --candidates t1 --tears calibrated
cargo run --release -p lp-store-bench -- double --candidates f2 --workloads save:c13
cargo run --release -p lp-store-bench -- random --candidates s1 --seeds 8 --steps 300
cargo run --release -p lp-store-bench -- measure --candidates f1,f2,s1,t1 --workloads 'push:c40;save:c40' --min-sectors
cargo run --release -p lp-store-bench -- endurance --candidates 'f1,f1@block_cycles=100' --corpus-name c20 --sectors 176 --pushes 0
cargo run --release -p lp-store-bench -- replay failure.json      # one scoreboard line
```

### Long histories, the edge, fuzzing and mutants (M3)

```bash
cargo run --release -p lp-store-bench -- long --candidates t1 --sectors 48 --corpora c13 --edit-mix \
  --tears clean,byte_prefix,random_bits,calibrated --piece-steps 5000 --checkpoint-dir <dir> --until +8m
cargo run --release -p lp-store-bench -- full-flash --candidates t1 --sectors 48 --corpus-name c13 --seeds 8
cargo run --release -p lp-store-bench -- fuzz --candidates t1 --cases 40000 --histories 16
cargo run --release -p lp-store-bench -- gc-dials --candidates t1
cargo run --release -p lp-store-bench --features mutants -- mutants [--only skip_kill,…]
```

- **`long`** (`driver_long.rs`): one store kept mounted for ≥ 100k steps,
  a full cut case every `--cut-every` steps, a full-state check and a
  remount every `--check-every`; `--edit-mix` fills the three slots once and
  then only saves, panel writes and re-pushes, so the live set sits near
  full and GC copies. Each walk reports its GC runs (the walk's own mounts),
  and stops at its first failure. `--piece-steps` + `--checkpoint-dir` +
  `--until` run it as resumable foreground pieces. The check also fails a
  walk on any program that asks a cleared bit to become 1
  (`program_over_unerased`, `lp-nor-sim`'s `violations_0_to_1`; weak bits a
  tear left are not counted): a store that programs cells it never read
  back as erased.
- **`full-flash`** (`driver_full_flash.rs`): unique copies of a project
  until the store refuses, then saves, panel writes, re-pushes and deletes
  at the edge with sampled cuts in every step, then space freed and a push
  again (`recovered`).
- **`fuzz`** (`driver_fuzz.rs`): mount on `garbage`, `mutated` (bit flips,
  0x00/0xFF runs, swapped or duplicated sectors, a scrambled body) and `cut`
  images, and for T1 `newer_version` (must refuse `Unsupported`) and
  `stale_tail` (**non-physical**: stale bytes in the erased tail of the two
  newest sectors, as a bad erase or a foreign writer would leave; it must
  mount at that committed state, take the next step without programming
  over them, and then format). A mounted image must read one committed
  state with no read error — littlefs mounts lazily and reports damage at
  read time, which this scores as `read_error_after_mount`.
- **`gc-dials`** (`driver_gc_dials.rs`): `gc_policy` × `reserve` × codec at
  partitions where c40 meets GC (host-deflated 28/36/48, stored 72/80/96).
- **`mutants`** (`driver_mutants.rs`, feature `mutants`): the unmutated
  store, then each of `lp-tree-store`'s eleven mutants, through one fixed set
  (`sweep`, `sweep_gc`, `double`, `long`, `full_flash`, `fuzz`, `wear` — a
  walk with one sector worn on erases and one on programs). A catch is a
  failure kind the unmutated store does not show in that driver; pass =
  every mutant caught. Last run (commit `f8257303b`, lp-nor-sim simulator):

  | mutant | caught first by | failures |
  |---|---|---|
  | `skip_kill` | wear | 1 (`unmountable`: a header reads newer) |
  | `trust_record_headers` | sweep | 708 |
  | `dedup_against_all_records` | long | 6 |
  | `gc_erase_before_copy` | sweep_gc | 359 |
  | `root_before_dirs` | double | 174 |
  | `resume_without_tail_check` | fuzz (`stale_tail`) | 70 |
  | `no_space_after_write` | sweep | 337 |
  | `gc_forgets_pending` | sweep | 104 |
  | `magic_first` | sweep_gc | 2 |
  | `newer_version_untrusted` | fuzz (`newer_version`) | 120 |
  | `open_without_read_back` | wear (`program_over_unerased`) | 2 |

  The unmutated store: 0 failures in all seven drivers (5,164 cases).

`overnight` also runs, every round, `long` (4 walks × 10k steps), one
`full-flash` and one `fuzz` per candidate (P8–P10) and the GC dials once
(P4b); `--tears` names the models every unit uses, `--rounds N` stops after
N rounds, and `--resume` skips the units the scoreboard already has and runs
each unit whole — the overnight run as foreground pieces.

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

`sweep`, `double` and `random` take `--tears` (comma-separated tear model
names; default `clean,byte_prefix,random_bits`, `lp-nor-sim`'s
`TearModel::ALL`; a walk draws each cut's model from the list). `smoke
--tears <names>` runs every driver of the smoke under those models; without
it the smoke is what it always was (the sweeps under the three guessed
models, the double cut under `random_bits`).
`calibrated` is the model measured on a real part (CX1, 500 cuts:
`docs/reports/2026-10-08-c6-nor-tear-calibration.md`) — torn erases that
read `0x00` from the front or throughout, or silently read erased; rarely a
cut that lands before an erase starts, or a program torn inside a 4-byte
word with a weak bit; it runs only when named, and a reproducer that names
it replays under it (under the weights of the build that replays it).
`calibrated_zeroing`, `calibrated_all_zero`, `calibrated_erasing`,
`calibrated_reads_ff_weak` and `calibrated_reads_ff` force every torn erase
to that one measured state (programs keep the calibrated mix);
`calibrated_before_erase` lands every erase cut before its erase and
`calibrated_mid_word` tears every program inside a word (the other family
keeps the mix). A sweep summary's `torn_erases` counts the cases whose cut
tore an erase (an erase cut before it started is not one).

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
| `sweep_summary` | `driver` (`exhaustive` / `double_cut`), `candidate`, `config`, `workload`, `tear`, `cases`, `landed`, `torn_erases`, `failures`, `non_atomic`, `kinds {failure kind: count}`, `steps_swept`, `steps_skipped`, `max_cuts_per_step`, `error` |
| `random_summary` | `candidate`, `config`, `seed`, `steps_run`, `steps_no_space`, `cuts`, `failures`, `non_atomic`, `kinds`, `first_failure`, `error` |
| `failure` | `driver`, `failure {kind, detail}`, `reproducer` (`{"case": {candidate, config, workload, step, cut_after, tear, seed, second}}` or `{"random": {…, stop_at_cut}}`, `{"long": …}`, `{"fuzz": …}`) — feed the line to `replay` |
| `long_summary` | `candidate`, `config`, `seed`, `edit_mix`, `steps_run`, `steps_no_space`, `cuts`, `landed`, `torn_erases`, `checks`, `failures`, `non_atomic`, `kinds`, `first_failure`, `gc {gc_runs, gc_copies, retired_sectors, erases_total, erases_max}`, `failed_case`, `error` |
| `full_flash_summary` | `candidate`, `config`, `corpus`, `seed`, `fill_slots`, `refusals`, `edge_steps`, `edge_refused`, `cases`, `refused_cases`, `landed`, `failures`, `kinds`, `recovered`, `gc` |
| `fuzz_summary` | `candidate`, `config`, `seed`, `cases`, `by_kind {kind: {cases, mounted, refused, failures}}`, `mutations`, `failures`, `kinds`, `first_failure`, `first_failing_case` |
| `gc_dial_row` | `candidate`, `config`, `workload`, `ok`, `gc_runs`, `gc_copies`, `write_amp`, `erases_{median,max}`, `cut_cases`, `cut_failures`, `cut_kinds` |
| `mutant_result` | `mutant` (`none` = unmutated), `drivers [{driver, cases, failures, kinds, new_kinds, first}]`, `failures`, `caught_by` |

At most 20 full failure records are written per (sweep, tear) and 5 per
random walk; the summaries count all of them.
