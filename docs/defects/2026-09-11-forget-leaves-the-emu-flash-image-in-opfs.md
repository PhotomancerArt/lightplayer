---
status: open      # a fix is in flight elsewhere (W5), not on this branch — do not claim fixed
found: 2026-09-11      # live-debugging, the C6-in-tab plan's G2
area: lpa-studio-web (public/lpa-link)
class: partial-knowledge-loss
related: [lp2025/2026-09-10-1707-c6-emulator-in-tab/G2-handoff.md]
---
# Forgetting an emu device removes the record but leaves its 4 MiB OPFS image behind

**Symptom** — reproduced cleanly, twice, on a board that had reached Ready:

```
images before Forget:  ["dev00000096yv62mg2m.bin"]
… Forget → Confirm Forget → the card is gone from the page …
images after Forget:   ["dev00000096yv62mg2m.bin"]
images after +4 s:     ["dev00000096yv62mg2m.bin"]
```

The card disappears and the record is gone from the roster; the flash image
stays in `emu-flash/` under OPFS forever.

**Root cause** — narrowed to `emulator_worker.js`'s `deleteFlash`, by
bisecting the chain from the console with every layer of Rust bypassed:

```js
const m = await import('/lpa-link/emulator_tab.js');
await m.deleteFlash('dev000000b6mxthsg1k');   // resolves, no error
// images after: ["dev000000b6mxthsg1k.bin"]  ← still there
```

`emulator_worker.js`'s `deleteFlash` (re-exported by `emulator_tab.js`)
resolves successfully and deletes nothing:

```js
export async function deleteFlash(key) {
  const root = await navigator.storage.getDirectory();
  const dir = await root.getDirectoryHandle(FLASH_DIR, { create: true });
  await dir.removeEntry(`${key}.bin`).catch(() => {});
}
```

The trailing `.catch(() => {})` swallows whatever `removeEntry` rejects
with. The controller path above it does its job correctly — `Forget` reads
the runtime's uid and kind before the fold, powers it off, and awaits
`EmuDeviceTransport::forget` → `delete_emu_flash` → this function — so the
gap is entirely inside the file above, not in the Rust effect chain calling
it.

**Plausible and untested** (from the G2 handoff that found this): the worker
still holds the file's sync access handle when the delete runs, and
`removeEntry` fails silently on a file with an open handle, exactly the
kind of failure the swallowed `.catch` would hide.

**Fix** — not made here. `emulator_worker.js` and `emulator_tab.js` are
files another effort (W5, part of
`lp2025/2026-09-11-0911-tab-emulator-loose-ends`) is editing concurrently
with this plan's P5; this phase was directed not to touch them. **Do not
treat this as fixed** until that work lands and this entry is updated.

**Regression coverage** — none yet; whoever fixes this should stop
swallowing the rejection (or explicitly close/release the sync access
handle before the delete) and add a test that a Forget after Ready leaves no
file behind.

**Lesson** — a bare `.catch(() => {})` on a delete is worse than no error
handling at all: it converts "this operation failed and here's why" into
"this operation succeeded", and the caller (Rust, the Forget effect) has no
way to know its promise of "the image is gone" was never kept. `Forget`
after this fix landed silently is a case worth re-testing directly against
OPFS state, not against the promise resolving.
