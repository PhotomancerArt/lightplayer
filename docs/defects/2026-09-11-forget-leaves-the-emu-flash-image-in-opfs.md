---
status: fixed
found: 2026-09-11      # live-debugging, the C6-in-tab plan's G2
fixed: 0560344fa       # #715 via #728 (P5b's merge, which carried #715)
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

**Fix** — landed 2026-09-13, PR #715 (W5), merged via #728 (`0560344fa`).
The cause was exactly the plausible-and-untested guess above: the worker
holds a sync access handle on the board's flash image for as long as the
board is alive, `removeEntry` refuses with `NoModificationAllowedError`
while that handle is open, and the bare `.catch(() => {})` swallowed the
rejection and turned it into a false success. The fix, in the two files
above:

- The worker's removal is now `removeFlashImage(key)` — it rejects by name
  when a file existed and did not go, rather than swallowing the error.
- `emulator_tab.js`'s `deleteFlash(key)` ends any live worker holding that
  key first, **awaiting** the `destroy` reply the worker sends only after
  it has written the chip back and closed the handle, then removes the
  file — sequencing, not a retry loop.

Verified in headless Chrome, real OPFS: before the fix, `deleteFlash` after
Forget resolved and the image stayed; after, `worker.removeFlashImage`
rejects `NoModificationAllowedError` while the handle is open, and
`tab.deleteFlash` (the Forget verb) resolves `true` and the image is gone.

**Regression coverage** — the before/after OPFS proof above (`node`,
headless Chrome, real `public/lpa-link/*.js`); no Rust test, because no Rust
changed.

**Lesson** — a bare `.catch(() => {})` on a delete is worse than no error
handling at all: it converts "this operation failed and here's why" into
"this operation succeeded", and the caller (Rust, the Forget effect) has no
way to know its promise of "the image is gone" was never kept. `Forget`
after this fix landed silently is a case worth re-testing directly against
OPFS state, not against the promise resolving.
