# M2: Static Slot Shape Registry Heap

## Goal

Stop paying per-project heap for static authored slot shape metadata that is the
same for every engine instance.

## Work

- Measure the heap delta of `Engine::with_services` and authored slot shape
  registration.
- Separate static built-in slot shapes from dynamic/project-local shape
  registration.
- Represent built-in shapes as shared/static data where possible.
- Keep lookup APIs stable enough that server/client slot introspection still
  works.
- Preserve a small dynamic overlay for shapes that truly are project-specific.

## Deliverables

- A registry representation with static built-ins and dynamic additions.
- Before/after memory numbers for engine creation and project load.
- Tests around authored slot shape lookup and any client-facing schema output.

## Validation

```bash
cargo check -p lpa-server
cargo test -p lpa-server --no-run
cargo test -p fw-tests --test profile_alloc_emu
```

## Implementation Strategy

Full plan. This likely touches model/engine API boundaries, and the right shape
should be written down before patching.
