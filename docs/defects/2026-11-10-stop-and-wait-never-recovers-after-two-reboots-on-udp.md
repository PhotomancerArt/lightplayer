---
status: open
found: 2026-11-10      # how: lp-link's delivery property on main's post-merge CI (Lint job, `test-rust-features`), a random proptest case
area: lp-link ARQ (StopAndWait) × board reboots × lp-link sim
class: untested-path
related:
  - ../adr/2026-09-27-lp-link-one-comms-layer.md
---
# Stop-and-wait never recovers after two board reboots on UDP

**Symptom** — `stop_and_wait_keeps_the_promise`
(`lp-base/lp-link/tests/delivery_properties.rs`) failed on main at
`aed89c689` ([run](https://github.com/PhotomancerArt/lightplayer/actions/runs/42768022317),
job "Lint (x64)"). Nothing on that commit touched lp-link: the property
draws a fresh random seed every run (`failure_persistence: None`), and this
run drew a case the variant cannot pass. After the quiet tail neither
direction had delivered anything for its last two generations, and both
senders still held unacknowledged data:

```
liveness host→board: generation (0, 1, 0) sent 6, delivered 0 after the quiet tail
liveness host→board: generation (0, 1, 1) sent 11, delivered 0 after the quiet tail
liveness host→board: the sender still holds unacknowledged data
liveness board→host: generation (2, 0, 0) sent 3, delivered 0 after the quiet tail
liveness board→host: generation (2, 0, 1) sent 15, delivered 0 after the quiet tail
liveness board→host: the sender still holds unacknowledged data
```

**Reproduce** — deterministic, `--features sim`:

```rust
let z = Faults { drop_packet: 0.0, drop_tail: 0.0, drop_write: 0.0, drop_span: 0.0,
                 corrupt: 0.0, duplicate: 0.0, spike: 0.0, spike_len: 0 };
let mut sc = Scenario::new(Transport::Udp, 0.0,
    Workload::Random { mean_gap: 8_000, max_size: 3_000 }, 1_500_000, 16758680932051761441);
sc.faults_up = Faults { drop_span: 0.0005503107119989484, ..z.clone() };
sc.faults_down = Faults { drop_tail: 0.07601779140123978, duplicate: 0.07456154374179952,
                          spike: 0.029259158621818267, spike_len: 314331, ..z };
sc.board_reboots = vec![430683, 421441];
assert!(run::<StopAndWait>(&sc).violations.is_empty()); // fails
```

Two board reboots about 9 ms apart, with duplicates, dropped
tails and long delay spikes on the board→host leg.

**Impact** — none on a product link: every product link runs
`SelectiveRepeat` or `NoArq`. `StopAndWait` is used only by `link-bench` and
the riscv32 size probe. Main's CI goes red whenever the property draws a
case like this one, which is rare. Re-running the job draws a new seed and
usually passes, and that hides the failure.

**Root cause** — not yet diagnosed. Liveness fails while safety holds (no
duplicate or out-of-order delivery is reported), so the likely suspect is
the stop-and-wait sender's state across a peer reset, when a late duplicate
from the old session arrives after the new SYN.

**Fix** — open. Either make stop-and-wait recover (and pin this case as a
regression test beside the property), or, if stop-and-wait is not worth
keeping, drop the variant from the property and the bench.
