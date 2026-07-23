---
status: fixed
found: 2026-07-22      # how: live-debugging
fixed: c9a75fa0e
area: fw-esp32/fs
class: backend-contract-divergence
related: ["2026-07-17-deletedir-error-shape", "LpFs conformance-suite chip"]
---
# fw-esp32 recursive list_dir doubled the base path

**Symptom** — Push to hardware failed with a path that reads like a
stutter:

```
Push failed: protocol error: hash package failed: filesystem:
File not found: /projects/studio/projects/studio/.lp
```

**Root cause** — The fw-esp32 recursive `list_dir` prepended the
listing prefix onto paths that littlefs already returns root-relative —
doubling the base for every entry of every non-root recursive listing.
The chroot view's prefix-strip then failed on the doubled paths,
breaking on-device `hash_package`. The memory-backed test filesystem
returns correctly shaped paths, so every e2e passed; only the littlefs
backend, which only runs on hardware, exhibited the doubling.

**Fix** — Paths are built as `"/"` + the root-relative lfs path,
instead of prefixing the listing dir onto an already-rooted path.

**Regression coverage** — None host-runnable today: fw-esp32 is a
bare-metal crate and its littlefs binding has no host harness. The LpFs
conformance-suite chip is the systemic answer — the same listing-shape
assertions run against every LpFs implementation, including the
firmware one, would have caught this before hardware did.

**Lesson** — Same lesson as `2026-07-17-deletedir-error-shape`, five
days later: two implementations of the LpFs contract disagreed on a
detail (path shape there, error shape here) that only real hardware
surfaces, because only the convenient backend is under test. The second
backend-divergence defect in a week is what triggered the
conformance-suite chip — and this registry. A contract with N
implementations and 1 tested implementation is tested at 1/N.
