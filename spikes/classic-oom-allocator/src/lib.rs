//! Host replay of the classic ESP32 OOM in
//! `docs/defects/2026-08-02-classic-oom-retry-succeeds.md`.
//!
//! The board reported a 3,072-byte allocation failing with `free=3508`,
//! `largest_free=3495` and `retry_ok=true`, which read as "the allocator
//! refused a request it could serve". These tests decide that against the real
//! allocator — `linked_list_allocator` 0.10.5, the version `esp-alloc` 0.10
//! pins — with no hardware in the loop.
//!
//! ⚠️ **`size_of::<Hole>()` is width-dependent**: 8 bytes on the 32-bit device,
//! 16 on a 64-bit host. Every size threshold below is expressed in terms of
//! [`min_size`] rather than a literal, so the same test states the same fact on
//! both. Where a device-specific number appears in a comment it has been halved
//! accordingly.

#![cfg(test)]

use linked_list_allocator::Heap;
use std::alloc::Layout;
use std::ptr::NonNull;

/// `fw-esp32v3`'s arena: `HEAP_SIZE = 110 * 1024`.
const ARENA: usize = 110 * 1024;

/// The allocator's minimum block, `size_of::<Hole>()` == `2 * size_of::<usize>()`.
fn min_size() -> usize {
    std::mem::size_of::<usize>() * 2
}

fn try_alloc(heap: &mut Heap, size: usize) -> bool {
    let layout = Layout::from_size_align(size, 4).unwrap();
    match heap.allocate_first_fit(layout) {
        Ok(ptr) => {
            unsafe { heap.deallocate(ptr, layout) };
            true
        }
        Err(_) => false,
    }
}

/// Port of `fw-esp32v3::recovery::panic_path::largest_free_block`.
fn largest_free_block(heap: &mut Heap) -> usize {
    const GRANULARITY: usize = 16;
    let mut too_big = heap.free() + 1;
    let mut fits = 0usize;
    while too_big - fits > GRANULARITY {
        let mid = fits + (too_big - fits) / 2;
        let Ok(layout) = Layout::from_size_align(mid, 4) else {
            break;
        };
        match heap.allocate_first_fit(layout) {
            Ok(ptr) => {
                unsafe { heap.deallocate(ptr, layout) };
                fits = mid;
            }
            Err(_) => too_big = mid,
        }
    }
    fits
}

/// Port of `fw-esp32v3::recovery::panic_path::free_list_shape`.
const MAX_RUNS: usize = 32;

#[derive(Debug, Clone, Copy)]
struct Shape {
    holes: usize,
    largest: usize,
    total: usize,
    truncated: bool,
}

fn free_list_shape(heap: &mut Heap) -> Shape {
    let step = min_size();
    let unit = Layout::from_size_align(1, 4).unwrap();
    let mut runs = [(0usize, 0usize); MAX_RUNS];
    let (mut n, mut last_end, mut truncated) = (0usize, 0usize, false);

    loop {
        let ptr = match heap.allocate_first_fit(unit) {
            Ok(p) => p.as_ptr() as usize,
            Err(_) => break,
        };
        if n > 0 && ptr == last_end {
            runs[n - 1].1 += step;
        } else {
            if n == MAX_RUNS {
                unsafe { heap.deallocate(NonNull::new(ptr as *mut u8).unwrap(), unit) };
                truncated = true;
                break;
            }
            runs[n] = (ptr, step);
            n += 1;
        }
        last_end = ptr + step;
    }

    let largest = runs[..n].iter().map(|&(_, l)| l).max().unwrap_or(0);
    let total = runs[..n].iter().map(|&(_, l)| l).sum();

    for &(start, len) in &runs[..n] {
        let mut off = 0;
        while off < len {
            unsafe { heap.deallocate(NonNull::new((start + off) as *mut u8).unwrap(), unit) };
            off += step;
        }
    }

    Shape { holes: n, largest, total, truncated }
}

/// A deterministic xorshift, so a failure is reproducible.
fn rng(seed: u64) -> impl FnMut() -> u64 {
    let mut s = seed;
    move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    }
}

/// Beat on a fresh heap until it is realistically fragmented.
fn churn(heap: &mut Heap, next: &mut impl FnMut() -> u64, steps: usize) -> Vec<(NonNull<u8>, Layout)> {
    let mut live: Vec<(NonNull<u8>, Layout)> = Vec::new();
    for _ in 0..steps {
        let r = next();
        if r % 3 == 0 && !live.is_empty() {
            let i = (next() as usize) % live.len();
            let (p, l) = live.swap_remove(i);
            unsafe { heap.deallocate(p, l) };
        } else {
            let layout = Layout::from_size_align(4 + (r as usize % 2048), 4).unwrap();
            if let Ok(p) = heap.allocate_first_fit(layout) {
                live.push((p, layout));
            }
        }
    }
    live
}

/// **The allocator is exonerated.** `split_current` refuses a hole only when the
/// leftover would be too small to record as a `Hole`, so the refusal window is
/// `(request, request + min_size]` and nothing wider. On the device that is
/// holes of 3,073..3,080 refusing a 3,072-byte request — nowhere near the
/// 3,495-byte hole the board reported having.
#[test]
fn refusal_window_is_at_most_one_hole_header_wide() {
    const REQ: usize = 3072;
    let min = min_size();
    let mut refused = Vec::new();

    for k in 0..=(4 * min) {
        let mut mem = vec![0u8; ARENA];
        let mut heap = unsafe { Heap::new(mem.as_mut_ptr(), ARENA) };
        let hole = REQ + k;
        // Carve exactly one hole of `hole` bytes, pinned on both sides.
        let head = Layout::from_size_align(ARENA - hole - 4 * min, 4).unwrap();
        let hp = heap.allocate_first_fit(head).unwrap();
        let victim = Layout::from_size_align(hole, 4).unwrap();
        let vp = heap.allocate_first_fit(victim).unwrap();
        let _ = heap.allocate_first_fit(Layout::from_size_align(1, 4).unwrap());
        unsafe { heap.deallocate(vp, victim) };

        if !try_alloc(&mut heap, REQ) {
            refused.push(hole);
        }
        unsafe { heap.deallocate(hp, head) };
    }

    assert!(!refused.is_empty(), "the refusal edge should exist at all");
    let widest = refused.iter().map(|h| h - REQ).max().unwrap();
    assert!(
        widest <= min,
        "refusal window is {widest} B wide, wider than one hole header ({min} B): {refused:?}"
    );
    // And the number from the board is comfortably outside it.
    assert!(REQ + widest < 3495, "a 3,495 B hole must serve a 3,072 B request");
}

/// **The probe does not manufacture `retry_ok`.** The OOM handler used to run
/// `largest_free_block()` — ~17 allocate/free round trips — before re-asking the
/// caller's question. For LLFF those round trips are inert, so the board's
/// `retry_ok=true` was a true statement. It is asserted here rather than assumed
/// because the handler's correctness depended on it, and because it stops being
/// true the moment anyone sets `ESP_ALLOC_CONFIG_HEAP_ALGORITHM=TLSF`.
///
/// The defect's headline figure (0 flips in 3,969,868 states) came from a longer
/// sweep; this runs a smaller one so `cargo test` stays quick.
#[test]
fn probe_never_flips_a_failing_request_to_succeeding() {
    let mut next = rng(0x2026_0802);
    let mut flips = 0usize;
    let mut examined = 0usize;

    for _ in 0..40u32 {
        let mut mem = vec![0u8; ARENA];
        let mut heap = unsafe { Heap::new(mem.as_mut_ptr(), ARENA) };
        let live = churn(&mut heap, &mut next, 3000);

        for s in (256..=8192).step_by(4) {
            if heap.free() <= s || try_alloc(&mut heap, s) {
                continue;
            }
            examined += 1;
            let _ = largest_free_block(&mut heap);
            if try_alloc(&mut heap, s) {
                flips += 1;
            }
        }
        for (p, l) in live {
            unsafe { heap.deallocate(p, l) };
        }
    }

    assert!(examined > 10_000, "only {examined} failing states examined");
    assert_eq!(flips, 0, "{flips} of {examined} failing requests flipped after the probe");
}

/// **`largest_free_block` bisects a non-monotonic predicate.** The refusal
/// window above means `alloc(S)` can fail where `alloc(S + 4)` succeeds, so its
/// answer is a floor, not a bound. This test asserts the counterexamples exist —
/// if they ever stop existing the doc comment on `largest_free_block` is stale
/// and the estimate could be promoted to a real bound.
#[test]
fn alloc_predicate_is_not_monotonic_in_size() {
    let mut next = rng(0xfeed_beef);
    let mut nonmono = 0usize;

    for _ in 0..40u32 {
        let mut mem = vec![0u8; ARENA];
        let mut heap = unsafe { Heap::new(mem.as_mut_ptr(), ARENA) };
        let live = churn(&mut heap, &mut next, 3000);

        let mut prev_ok = true;
        for s in (16..=heap.free().min(8192)).step_by(4) {
            let ok = try_alloc(&mut heap, s);
            if ok && !prev_ok {
                nonmono += 1;
            }
            prev_ok = ok;
        }
        for (p, l) in live {
            unsafe { heap.deallocate(p, l) };
        }
    }

    assert!(
        nonmono > 0,
        "found no S where alloc(S) fails and alloc(S+4) succeeds; \
         largest_free_block's bisection may now be sound"
    );
}

/// **The exact walk is exact, and puts every byte back.** This is the algorithm
/// `fw-esp32v3` now runs in its OOM handler, where a bug means losing the crash
/// report entirely — so it is tested against known free lists rather than
/// trusted.
#[test]
fn free_list_shape_is_exact_and_restores_the_heap() {
    let min = min_size();
    for want_holes in [1usize, 2, 5, 17] {
        let mut mem = vec![0u8; ARENA];
        let mut heap = unsafe { Heap::new(mem.as_mut_ptr(), ARENA) };
        let victim = Layout::from_size_align(1024, 4).unwrap();
        let pin = Layout::from_size_align(64, 4).unwrap();

        let mut victims = Vec::new();
        for _ in 0..want_holes {
            victims.push(heap.allocate_first_fit(victim).unwrap());
            let _ = heap.allocate_first_fit(pin).unwrap();
        }
        // Consume the tail so it is not an extra hole.
        while heap.allocate_first_fit(Layout::from_size_align(512, 4).unwrap()).is_ok() {}
        while heap.allocate_first_fit(Layout::from_size_align(1, 4).unwrap()).is_ok() {}
        for v in &victims {
            unsafe { heap.deallocate(*v, victim) };
        }

        let free_before = heap.free();
        let shape = free_list_shape(&mut heap);

        assert_eq!(shape.holes, want_holes, "hole count");
        assert_eq!(shape.largest, 1024, "largest hole");
        assert!(!shape.truncated);
        assert_eq!(heap.free(), free_before, "the walk must give every byte back");
        // Each hole is walked in `min_size` steps, so `total` can sit up to
        // `min_size - 4` bytes below each hole's true size.
        assert!(shape.total <= free_before);
        assert!(shape.total + min * shape.holes >= free_before);
    }
}

/// Truncation is the dangerous path: it stops mid-walk, and it still has to
/// return everything it took.
#[test]
fn free_list_shape_restores_the_heap_when_truncated() {
    let mut mem = vec![0u8; ARENA];
    let mut heap = unsafe { Heap::new(mem.as_mut_ptr(), ARENA) };
    let victim = Layout::from_size_align(256, 4).unwrap();
    let pin = Layout::from_size_align(64, 4).unwrap();

    let mut victims = Vec::new();
    for _ in 0..(MAX_RUNS + 20) {
        victims.push(heap.allocate_first_fit(victim).unwrap());
        let _ = heap.allocate_first_fit(pin).unwrap();
    }
    for v in &victims {
        unsafe { heap.deallocate(*v, victim) };
    }

    let free_before = heap.free();
    let shape = free_list_shape(&mut heap);

    assert!(shape.truncated, "more holes than MAX_RUNS should truncate");
    assert_eq!(shape.holes, MAX_RUNS);
    assert_eq!(heap.free(), free_before, "truncated walk must still give every byte back");
}

/// The whole point of replacing the bisection on the OOM path: on randomised
/// heaps the walk's `largest` equals the exhaustively-probed truth exactly,
/// where the bisection is only within its 16-byte granularity.
#[test]
fn walk_beats_bisection_on_accuracy() {
    let mut next = rng(0xa5a5_1234);

    for _ in 0..20u32 {
        let mut mem = vec![0u8; ARENA];
        let mut heap = unsafe { Heap::new(mem.as_mut_ptr(), ARENA) };
        let live = churn(&mut heap, &mut next, 3000);

        // Ground truth: the largest size that actually allocates.
        let mut truth = 0usize;
        for s in (min_size()..=heap.free()).step_by(4) {
            if try_alloc(&mut heap, s) {
                truth = s;
            }
        }

        let shape = free_list_shape(&mut heap);
        if !shape.truncated {
            assert_eq!(shape.largest, truth, "walk should be exact");
        }
        let est = largest_free_block(&mut heap);
        assert!(est <= truth, "the estimate must never over-report");

        for (p, l) in live {
            unsafe { heap.deallocate(p, l) };
        }
    }
}
