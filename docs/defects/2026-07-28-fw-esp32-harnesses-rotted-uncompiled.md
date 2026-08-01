---
status: fixed
found: 2026-07-28      # how: report (reached for during hardware-walk prep)
fixed: this change
area: lp-fw/fw-esp32c6 (src/tests/ harnesses + module cfg gates)
class: ungated-variant
related: [2026-07-28-esp32c6-app-partition-overflow.md]
---
# Three fw-esp32 hardware harnesses had rotted because no gate ever compiled them

**Symptom** — reaching for a GPIO walk on the C6 produced a build failure, not
a build. On `ff5c4eed4`, `cargo check --features test_gpio,esp32c6` reported 26
errors: `an "extern crate" loading macros must be at the crate root`, twenty-two
`expected GPIO<'_>, found GPIO0<'_>` mismatches, `associated function "take" is
private`, and `no method named "write" found for RefMut<'_, Esp32UsbSerialIo>`.
`--features test_json,esp32c6` reported `missing field "recovery" in initializer
of ServerMsgBody`; `--features test_usb,esp32c6` reported `expected
SpawnToken<_>, found Result<SpawnToken<impl Sized>, SpawnError>`. Nothing had
touched these files — the tree around them had moved.

**Root cause** — each harness feature sets `fw_harness` and replaces `main` with
its own entrypoint, so the harness sources compile in *no other configuration*.
`build-fw-esp32c6` and `clippy-fw-esp32c6` both build only `--features esp32c6`, and
the CI matrix inherited that shape. Every upstream change since the harnesses
were last run therefore landed on them unobserved: esp-hal 1.1 made
`Peripherals::take()` private, gave each pin its own `GPIOn` type, and made
`UsbSerialJtag::into_async()` incompatible with the `Blocking` handle
`Esp32UsbSerialIo::new` takes; `logger::LogWriteFn` narrowed to a bare `fn(&str)`
that a capturing closure can no longer coerce to; embassy-executor 0.10 made
task constructors return `Result<SpawnToken<_>, _>`; and `ServerMsgBody::Heartbeat`
gained a `recovery` field. Each of those was a correct, complete change to
everything the build system could see. The harnesses were simply outside it.

The same blindness ran the other way through the cfg gates. Several module gates
named harnesses that never used the module — `hardware` was compiled for
`test_shader_compile_incremental`, `usb_connection` for `test_jit_math_perf` —
and nothing ever noticed, because no build with those features was ever
type-checked, let alone linted.

**Fix** — repaired the three harnesses against the current APIs (`test_gpio` now
opens pins via `AnyPin::steal` after dropping the board-owned handles, and
publishes the serial handle to the logger with `set_log_serial` instead of
capturing it in a closure); tightened each module gate in `hardware/mod.rs`,
`board/esp32c6/mod.rs`, and `main.rs` to exactly its callers; and added
`just clippy-fw-esp32c6-harnesses`, which clippies all thirteen harness feature
combinations at `-D warnings` and is a dependency of `clippy-rv32`, so it runs
inside both `just check` and the CI lint job. Also added the three
`fwtest-{gpio,button,usb}-esp32c6` run recipes, which had never existed.

The `fw-esp32` → `fw-esp32c6` + `fw-esp32-common` split (#180) landed while this
was in flight, and the gate immediately earned its keep on the merge: the split
had gated `serial::Esp32UsbSerialIo`'s re-export on bare `fw_harness`, which is
too broad — five of the thirteen harnesses log through `esp_println` or `io_task`
and never name the type, so it read as an unused import in each. That is exactly
the class of drift the split could not have caught on its own, and the gate
caught it on the first run.

**Regression coverage** — `just clippy-fw-esp32c6-harnesses` (13 combos; ~13s
warm). It is a compile gate, not a behavioral one: it proves the harnesses
build, not that they still do the right thing on silicon.

**Hardware walk (esp32c6 rev v0.2, 2026-07-28)** — all three verified on
silicon. `test_json` streams valid `M!` heartbeat JSON at 1 Hz carrying the new
`"recovery":null`; `test_usb` spawns its heartbeat task and counts ~970
frames/s; `test_gpio` runs 8 consecutive clean cycles over all 20 configured
pins with USB alive throughout.

Getting there exposed a second, older bug in the first rewrite of `test_gpio`.
Making `GPIO_PINS_TO_TEST` authoritative (the old match arm skipped GPIO13 even
though the constant listed it) meant GPIO13 got driven — and **GPIO12/GPIO13 are
USB_D-/USB_D+ on the C6**, so the port vanished from the host mid-run and the
board needed a physical replug. Both pins were *already* marked reserved in
`boards/seeed/xiao-esp32-c6.json` ("crashed or timed out during calibration");
the harness and the calibration manifest had simply never been reconciled. Both
are now excluded here and in `test_gpio_calibrate`'s `supports_gpio`.

GPIO20 was briefly suspected of the same fault and is **not** — it was a false
positive read off a session with three ESP boards enumerated at once, repeated
espflash reconnect failures, and a replug mid-run. Retested alone on a quiet bus,
it passes. Worth recording as its own small lesson: on a wedged USB link, the
next observation is not evidence until the bus is quiet and the board identified
(`espflash board-info`) — the second C6 on the desk was not even the same board.

**Lesson** — a build configuration that nothing compiles is not "untested code",
it is *unwritten* code that happens to have a file. Test coverage gaps degrade
gracefully — the code still builds, so refactors still reach it. Compile-coverage
gaps do not: the code silently falls out of the language's own consistency
checking, and the cost lands entirely on whoever next needs the thing, at the
worst moment (a device on the desk, a walk to run). The rule that follows is
narrow and mechanical: **every feature combination the repo offers a way to run
must be a combination some gate builds.** The tell here was structural and
findable without any bug — thirteen `fwtest-*` recipes, one `build-fw-esp32c6`.
Whenever a justfile can invoke more configurations than CI compiles, the
difference is rot in waiting.
