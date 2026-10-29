# ADR: Install identifies the board first; the board model is asked after

- **Status:** Accepted (Yona, 2026-10-28: "yes. lets do it. I trust you to plan, impl, and ship")
- **Date:** 2026-10-28
- **Deciders:** Yona, with Sean's report
- **Supersedes:** None
- **Superseded by:** None
- **Refines:** [2026-08-02-flash-image-selected-from-the-discovered-chip](2026-08-02-flash-image-selected-from-the-discovered-chip.md)
  (the chip still decides the image; now the bootloader session that writes
  it is where the chip is learned)
- **Related:** [2026-10-01-agentic-control-offers-in-core](2026-10-01-agentic-control-offers-in-core.md),
  [2026-10-01-offer-tree-and-consequence-levels](2026-10-01-offer-tree-and-consequence-levels.md).
  Planning: `lp2025/2026-10-28-0025-install-without-board-pick` (`plan.md`,
  `notes.md`).

## Context

Installing LightPlayer made people pick their board first, from a list that
was often not even narrowed to their chip. Sean hit it with a WLED
controller. The list was unfiltered because Studio asked before it had
talked to the board: the chip was learned only passively, from a boot banner
or a hello. A board plugged in mid-stream, or one running a quiet firmware,
never says.

Yet the flash itself already opens a session with the chip's ROM
bootloader, and that session is the one place the chip is certain: its SYNC
names the silicon. Only three firmware builds exist, one per chip
(`lp-fw/builds/`). The board model never chose the firmware. It only chose
the pin map Studio writes to `/hardware.json` after the flash.

## Decision

1. **The package is chosen inside the bootloader session, by the chip it
   hears.** The flash's layout inspection, which already runs before every
   write and leaves the chip parked, takes a `FlashBuild`: a fixed build, or
   the chip table (`lpa_boards::builds_by_chip`). It picks the package after
   SYNC, through a Rust callback, and says which it chose. No new session
   and no new reset. A chip the table lacks stops the install with nothing
   written. The flash-time chip guard still checks the package against the
   silicon. When Studio already heard the chip, the press carries that
   chip's build as a fixed one. Sims and emulated tab boards read nothing in
   a bootloader, and need it that way.

2. **The same session reads what the board runs.** It reads the partition
   table (already the layout probe's first read), and, for a table that is
   not LightPlayer's, the first app's `esp_app_desc_t`.
   - Erased: blank.
   - An `lpfs` partition: LightPlayer.
   - Otherwise: somebody else's firmware, as it names itself, or
     unrecognised.

   It also reads the flash chip's JEDEC id for the board's flash size.

3. **Install is Routine at the press. The one question it asks is inside
   the activity.** Replacing somebody else's firmware (or one that does not
   say what it is) waits on `ConfirmFlashReplace`. That answer is Lasting:
   the agent never presses it, and its words name what the board runs
   ("This board is running WLED 0.15.0."). Blank and LightPlayer boards are
   never asked.

4. **The board model is a refinement, asked after when it must be.** The
   effects layer owns the catalog. It matches a fingerprint (chip, flash
   size, USB vendor; `lpa_boards::match_boards`). It stamps the model only
   when exactly one catalog board fits, or when the board already named one.
   Otherwise it stamps nothing, and the card's Hardware row asks "Which
   board is this?" (`set-board`, Routine). Answering writes `/hardware.json`
   and restarts the board. The same verb sits in the details as "Change
   board" whenever the chip fits several. No firmware verb takes a `board`
   parameter.

5. **An install the agent started that stops on the user's question resumes
   the agent at once, with the question**, rather than at its deadline.

## Alternatives rejected

- **A custom RAM payload to fingerprint the board.** The ROM, the esptool
  stub and the USB descriptors already give the chip, MAC, flash size and
  bridge. Board identity is not readable from silicon, and probing pins on
  unknown hardware risks what is wired to them. It would also mean one more
  per-chip program to keep licence-clean.
- **A separate identify session before the inspection.** It costs a reset
  and, on native USB, a re-enumeration per install, which is the hazard the
  flash ladder exists to absorb.
- **Scanning the image for a product name.** That would name WLED even in
  an Arduino build. But it reads megabytes over a link that manages about
  12 KB/s on a Mac. The app description is about 300 bytes away, and an
  Arduino build says "an Arduino sketch".
- **Asking the board model before writing when several fit.** The firmware
  is identical across them, and the answer is a file the next answer
  replaces (D1).

## Consequences

- One press installs a blank board, whatever Studio heard before.
- esptool-js's flash-id probe can read 0 on an ESP32-C6 rev 2 over
  USB-Serial-JTAG (bench, 2026-07-31). Until that probe works there, a XIAO
  ESP32-C6 and a C6 DevKitC-1 both fit, and such a board is asked which it
  is after installing. A board that already named its model (any earlier
  stamp) is not asked.
- Classic boards behind the same bridge vendor fit several catalog boards,
  and are asked after. Real USB product ids per bridge would narrow them
  further.
- The fake board in Studio's tests now answers the bootloader as the chip
  its script plays, with a 4 MB flash id, the bridge that chip sits behind,
  and a real ESP-IDF layout for its foreign firmware.
- No device wire and no persisted format changed. `/hardware.json` bytes
  are unchanged. The journal's `Action::Flash` grew optional fields.
