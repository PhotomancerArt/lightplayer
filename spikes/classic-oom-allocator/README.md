# classic-oom-allocator

Host replay of the classic ESP32 OOM in
[`docs/defects/2026-08-02-classic-oom-retry-succeeds.md`](../../docs/defects/2026-08-02-classic-oom-retry-succeeds.md).

```bash
cd spikes/classic-oom-allocator && cargo test --release
```

The board reported a 3,072-byte allocation failing with `free=3508`,
`largest_free=3495`, `retry_ok=true`. That read as "the allocator refused a
request it could serve", and the filed hypothesis was a first-fit edge in
`linked_list_allocator` that the OOM probe's own allocate/free pair had
normalised away before the retry.

These tests decide it against the real allocator, pinned to the version
`esp-alloc` 0.10 pins, with no board in the loop:

| test | what it settles |
|---|---|
| `refusal_window_is_at_most_one_hole_header_wide` | the refusal edge is real but only `(request, request + size_of::<Hole>()]` wide — a 3,495 B hole always serves 3,072 B, so **the allocator is exonerated** |
| `probe_never_flips_a_failing_request_to_succeeding` | allocate/free round trips are inert for LLFF, so `retry_ok=true` was genuine — **the hypothesis is refuted** |
| `alloc_predicate_is_not_monotonic_in_size` | `largest_free_block`'s bisection is unsound in principle; its answer is a floor |
| `free_list_shape_*` | the exact walk now used on the OOM path reports the true free list and gives every byte back, including when truncated |
| `walk_beats_bisection_on_accuracy` | the walk is exact where the bisection is only within 16 B |

Conclusion: if the heap at the instant of failure had looked like the printed
numbers, the request would have been served. It was not — so the report
describes a heap the failing allocation never saw, and the remaining question is
what changed in between.

Not a workspace member: this is an investigation artifact, not shipped code, and
it should not gate CI.

⚠️ `size_of::<Hole>()` is 8 bytes on the 32-bit device and 16 on a 64-bit host,
so every threshold is written in terms of it rather than as a literal. Run this
on a 64-bit host and the *widths* double; the facts do not.
