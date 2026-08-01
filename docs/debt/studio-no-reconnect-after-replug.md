---
status: carried
since: 2026-07-31      # boot_safe_once began ending on the replug instruction
logged: 2026-08-01
area: lpa-link/browser-serial + studio device cards
related:
  [
    "web-serial-js-untestable.md",
    "safe-mode-dim-boot-unproven.md",
  ]
---
# Studio never reconnects after a physical replug

**Shape** — Bootloader-mode ops (arm safe mode, wipe) end on "Unplug
the board and plug it back in to start it" — a *physical* replug is the
only way a C6 over USB-Serial-JTAG leaves download mode. But nothing on
the Studio side re-attaches when the device re-enumerates:
`browser_serial.js` registers a serial `connect` listener, yet the
awaiting op card sits at "Waiting for device output…" forever, and no
`[device]` lines appear after the replug. The awaiting op is also never
cleared by a later successful connect (verify who clears
`device_card_op` before building on it).

**Carrying cost** — Every recovery flow ends on an instruction the app
cannot confirm was followed. The user replugs, the board boots, and
Studio looks broken. This also blocks proving the dim-boot leg from
inside Studio (see `safe-mode-dim-boot-unproven.md`).

**Workarounds** — Reload the Studio tab after replugging and reconnect
through the normal device picker; the Web Serial permission grant
survives, so this is clicks, not re-pairing.

**Exit criteria** — After a bootloader-mode op ends awaiting replug, a
re-enumerated granted port is re-opened automatically (or one click),
the op card resolves, and `[device]` output flows. Note Web Serial
`connect` events only fire for previously-granted ports — the grant
retention in `browser_serial.js` is the hook. Testing falls under the
`web-serial-js-untestable.md` gap: expect this to need a hardware walk.
