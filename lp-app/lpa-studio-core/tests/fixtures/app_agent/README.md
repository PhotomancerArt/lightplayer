# App-agent eval fixtures

The headline evals for the app-level agent (plan
`lp2025/2026-10-01-0126-app-agent-harness`): can a model build Sean's
project — *"set up my leds to do some simple fun rainbow patterns. I have
250 leds on D6."* — from an empty project, and does it light?

Two stages, so each runs where its tools are:

- **Stage A** — `lpa-studio-core`, `src/app/agent/evals/`. A headless Studio
  over an in-process server wearing the **XIAO ESP32-C6's real pin map**
  runs the scenario, then pure checks judge the project it left. Writes
  `target/app-agent-evals/<run>/<scenario>/{project/,transcript.json,report.json}`.
- **Stage B** — `lp-cli/tests/app_agent_emu_decode.rs`. The project tree is
  deployed to the shipped `fw-esp32c6` image in the emulator, and the frames
  on **GPIO16 (XIAO D6)** are decoded: whole, error-free, the right LED
  count, lit, and changing over time.

## E4: the device journey (deterministic only)

E1–E3 build a project; E4 asks whether the agent can get a board to run
one: connect a board, flash a **blank** board, push a project, and see it
run — every step an `act` on a device offer by its path, with its values in
`args` (`devices/connect-usb`, `devices/new-<n>/flash` with `board`,
`devices/mac-<hex>/push` with `source`). Two tests, in
`src/app/studio/studio_device_e2e_tests/agent_device_journey_tests.rs`:

- `e4_the_agent_connects_flashes_a_blank_board_pushes_and_sees_it_run` —
  the connect press is a card (the browser's chooser needs a real click) and
  opens no port until the user's click; the blank board's Flash is Routine,
  so the agent's press flashes the board it named; the push sends the
  gallery's example; the readout of the agent's last turn says the board
  runs it.
- `e4_the_agents_flash_over_firmware_is_a_card_and_flashes_nothing` — a
  board running other firmware: the agent's Flash is Lasting, so it becomes
  a card pre-filled with the agent's board, a second press while the card
  waits is refused, and nothing is flashed.

**The seam.** E4 is not a scenario TOML and has no live leg. The eval
driver above has no device transport, so E4 seats the same app chat on the
device e2e bench (`DeviceBench`) instead. Only two things are fake:

- **The model.** The evals' `ScriptedProvider` plays fixed turns.
  Everything after it is the product's code: the run, the `act` tool, the
  host bridge's `AgentOp::AppAct` on the command queue, and the
  controller's offer press. A small seat applies the run's commands and
  refreshes the readout after each batch, as the actor does.
- **The board.** `FakeEsp32Device` behind the bench's scripted USB
  transport. Its flash is a scripted transition to LightPlayer (with a
  heartbeat, so it reports what it runs), and the preflight "reads" a fixed
  MAC (`60:55:f9:0a:0b:0c`). After the flash it speaks the real wire to a
  real host `LpServer`, so the push is the real conversation.

**What an emulated C6 would add** (`lp-emu-esp32c6`, as `just walk-no-board`
uses):

- the shipped `fw-esp32c6` bytes written to emulated flash and booted
  through the ROM and bootloader, rather than a state change;
- the MAC read from emulated eFuse;
- the hello and heartbeat from real firmware over `lp-link` on the emulated
  USB-Serial-JTAG;
- the project compiled by the device's own JIT and rendered to the LED pin,
  rather than a host server reporting it loaded.

That walk proves the same journey by hand-driven UI, with no agent in it.

```bash
cargo test -p lpa-studio-core e4_
```

## Layout

| Path | What |
|---|---|
| `scenarios/<name>.toml` | One eval each (E1 Sean from empty, E2 make it 300, E3 never guess the board). |
| `golden/<name>/` | Hand-built project trees that pass their scenario. The vendored `modules/*` are catalog `effect/` folders byte for byte. |

## Scenario fields

| Field | Meaning |
|---|---|
| `name` | Must equal the file stem. |
| `summary` | One line for the report. |
| `start` | `"blank"` (a new Blank project) or `{ golden = "<name>" }` (that golden, open and saved). |
| `context` | What the app knows before the user speaks (the board, the connected device). |
| `user` | The user's message. |
| `leds` | The LED count the project should end with (`strip_of`, stage B). |
| `golden` | The golden that proves this scenario's checks pass. |
| `replies` | `[[replies]] when_asked_about = "board", text = "…"`: the agent ending a turn with a question about that topic gets the next unused reply; a question with none left ends the run. |
| `budget` | `{ turns, usd }` for the whole scenario. |
| `playlist` | `min_entries`, `step_seconds = [lo, hi]`, and `colourful` — the catalog slugs that count (edit freely). |
| `checks` | Which checks decide pass/fail, in report order. |

## Checks

`output_on_d6`, `target_is_xiao_c6`, `strip_of` (exactly `leds` lamps in
strip order, a render size that gives each its own texel),
`playlist_cycles` (a Cycle playlist, step in range, enough entries whose
shader source is a colourful catalog pattern's), `graph_wired` (clock →
playlist → fixture → output over the bus), `all_nodes_ok` (no node failed,
something runs), `saved`, `asked_about_board`, `no_d_label_before_board`
(no `ws281x:local:D<n>` written before the board reply), `minimal_diff`
(from the start project only the strip size changed). Each failure names
what it saw.

## Running

```bash
# Deterministic legs (CI): goldens pass, negatives fail.
cargo test -p lpa-studio-core app_agent
cargo test -p lpa-studio-core e4_      # the device journey (above)
just test-emu-c6-cli          # includes stage B on both goldens

# Stage B alone against CI's images (no firmware build):
just fetch-ci-images          # prints export LP_CI_IMAGES=…
scripts/ci/ci-images.py with esp32c6 -- \
  cargo test -p lp-cli --release --test app_agent_emu_decode -- --include-ignored --nocapture the_sean

# Live leg — costs money, never CI:
just app-agent-eval e1 --model <openrouter slug>
just app-agent-eval all --model <slug> --repeat 3
```

The live leg reads the key from `OPENROUTER_API_KEY` or
`~/.lightplayer/settings.json` (`agent.openrouter_api_key`) and never writes
it anywhere. Emulated numbers are `lp-emu:esp32c6:t1` with the `lp-emu`
commit printed beside them; they are not hardware-validated.
