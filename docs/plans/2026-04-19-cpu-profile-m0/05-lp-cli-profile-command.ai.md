# Phase 5 — New `lp-cli profile` command + diff stub

Add the unified `lp-cli profile` command described in the design
doc. After this phase, **`lp-cli profile --collect alloc` is the
preferred path**, and `mem-profile`/`heap-summary` are still alive
but redundant (deleted in phase 6).

Depends on phases 3 (AllocCollector exists) and 4
(`with_profile_session` builder exists). Independent of phase 2
once that's merged.

## Subagent assignment

`generalPurpose` subagent. Mostly mechanical port of
`mem_profile/handler.rs` to the new module structure, plus a
small new args/diff-stub layer.

## Files to create

```
lp-cli/src/commands/
└── profile/
    ├── mod.rs        # NEW: re-exports + module wiring
    ├── args.rs       # NEW: ProfileArgs, ProfileDiffArgs
    ├── handler.rs    # NEW: profile run handler
    └── diff_stub.rs  # NEW: profile diff stub
```

## Files to update

```
lp-cli/src/commands/mod.rs   # add `pub mod profile;`
lp-cli/src/main.rs           # register new subcommands
```

## `args.rs`

Use whatever clap derive style the existing commands use
(check `commands/mem_profile/args.rs` for the pattern). Define:

```rust
#[derive(Debug, Args)]
pub struct ProfileArgs {
    /// Workload directory (defaults to examples/basic).
    #[arg(default_value = "examples/basic")]
    pub dir: PathBuf,

    /// Collectors to enable (comma-separated).
    /// m0 supports: alloc.
    #[arg(long, default_value = "alloc", value_delimiter = ',')]
    pub collect: Vec<String>,

    /// Number of frames to advance the workload.
    #[arg(long, default_value_t = 10)]
    pub frames: u32,

    /// Optional human-readable note appended to the profile dir.
    #[arg(long)]
    pub note: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProfileDiffArgs {
    pub a: PathBuf,
    pub b: PathBuf,
}
```

Validation: in `handler.rs` (or a small helper in `args.rs`),
reject any value in `--collect` other than `alloc` with a clear
error message. The list is small enough to hand-validate; no need
for an enum yet (m1 introduces the events/cpu collectors).

## `handler.rs`

Port `mem_profile/handler.rs` end-to-end with these changes:

1. **Output dir**:
   - Old: `traces/<timestamp>--<workload>/`.
   - New: `profiles/<timestamp>--<workload>[--<note>]/`.
   - `<workload>` is `args.dir` flattened with `/` → `-`,
     same rule the old code uses; check `mem_profile/handler.rs`
     for the exact transform and reuse it.
   - `<note>` is appended only if `args.note` is `Some`.

2. **Build features**: pass `--features profile` (renamed in
   phase 2). The old handler is already updated to do this in
   phase 2; the new handler does the same.

3. **SessionMetadata assembly**:

   ```rust
   let metadata = SessionMetadata {
       schema_version: 1,
       timestamp: timestamp_str.clone(),
       project: project_uid,             // however mem_profile gets it today
       workload: args.dir.display().to_string(),
       note: args.note.clone(),
       clock_source: "emu_estimated",
       frames_requested: args.frames,
       symbols: extracted_symbols,       // from ELF, same as mem_profile today
   };
   ```

4. **Collectors**:

   ```rust
   let mut collectors: Vec<Box<dyn Collector>> = Vec::new();
   for name in &args.collect {
       match name.as_str() {
           "alloc" => collectors.push(Box::new(
               AllocCollector::new(&trace_dir, heap_start, heap_size)?
           )),
           other => bail!("unknown collector '{other}'; supported: alloc"),
       }
   }
   ```

5. **Emulator construction**: use `with_profile_session` instead
   of `with_alloc_trace`. `finish_profile_session` instead of
   `finish_alloc_trace`.

6. **Final logging**: same shape as `mem_profile` today (event
   count, output path), but read alloc event count via the
   per-collector getter (since `finish_profile_session` returns
   aggregate). Easiest: keep a typed handle to the
   `AllocCollector` before boxing it OR hold an `Arc<Mutex<u64>>`
   counter — simpler still: just `wc -l` the `heap-trace.jsonl`
   after the fact, since this is just a log line.

   **Recommended approach**: have `ProfileSession::finish` return
   `Vec<(String, u64)>` of `(collector_name, event_count)` —
   small refactor against phase-1's stubbed return value, worth
   it for clean logging here. Update phase 1's note accordingly
   (or just keep that as a TODO and resolve here).

## `diff_stub.rs`

```rust
use std::process::ExitCode;
use super::args::ProfileDiffArgs;

pub fn handle_profile_diff(args: ProfileDiffArgs) -> ExitCode {
    eprintln!("error: 'lp-cli profile diff' is not yet implemented (planned for cpu-profile m2)");
    eprintln!("trace dirs: {}, {}", args.a.display(), args.b.display());
    ExitCode::from(2)
}
```

(Use `ExitCode` rather than `std::process::exit` so the rest of
the CLI's error handling stays consistent. If the codebase pattern
is `exit(N)`, match that instead.)

## `mod.rs` (commands/profile/)

```rust
pub mod args;
pub mod diff_stub;
pub mod handler;

pub use args::{ProfileArgs, ProfileDiffArgs};
pub use diff_stub::handle_profile_diff;
pub use handler::handle_profile;
```

## `main.rs` wiring

Find the clap `Commands` enum. Add:

```rust
/// Run a profiling session against a workload.
Profile(ProfileArgs),
/// Compare two profile directories. (m0: stub)
ProfileDiff(ProfileDiffArgs),
```

…or use clap's subcommand grouping if the existing pattern is
`profile { run, diff }` style — match the codebase's existing
convention (check whether `mem-profile` is a flat command or a
subcommand group).

If clap supports it cleanly, prefer `profile` with nested
subcommands `run` (default) and `diff`:

```
lp-cli profile [DIR]            # implicitly run
lp-cli profile diff <a> <b>
```

Match the dispatch in main.rs accordingly.

**Do not remove `MemProfile` and `HeapSummary` registrations
in this phase** — they stay alive until phase 6. The two paths
coexist briefly, both pointing at the same underlying
infrastructure (since phase 4 routed `with_alloc_trace` through
`ProfileSession`).

## Validation

```bash
cargo check -p lp-cli
cargo build -p lp-cli

# Smoke run (no assertions yet — phase 7 adds the test)
cargo run -p lp-cli -- profile examples/basic --collect alloc --frames 2 --note manual
ls profiles/
#   2026-XX-XX...--examples-basic--manual/
ls profiles/*--examples-basic--manual/
#   meta.json  heap-trace.jsonl  report.txt

# Diff stub fails non-zero with informative stderr
cargo run -p lp-cli -- profile diff /tmp/a /tmp/b ; echo "exit=$?"
#   → "exit=2"

# Old commands still work (deleted in phase 6)
cargo run -p lp-cli -- mem-profile examples/basic
cargo run -p lp-cli -- heap-summary traces/<latest>
```

Inspect `profiles/.../meta.json` and verify the structure matches
the design doc:

- top-level: `schema_version`, `timestamp`, `project`, `workload`,
  `note`, `clock_source`, `frames_requested`, `symbols`,
  `collectors`.
- `collectors.alloc`: `heap_start`, `heap_size`.

Inspect `report.txt` and verify it starts with
`=== Heap Allocation ===` and contains the same body content as
the old `heap-summary` output (minus its top-level header).

## Out of scope for this phase

- Deleting `mem_profile/`, `heap_summary/`, `alloc_trace.rs` (phase 6).
- Renaming `examples/mem-profile/` (phase 8).
- justfile recipe updates (phase 8).
- Test coverage for the new command (phase 7).
