# App-agent evals and the agent activity corpus

The **corpus** (plan `lp2025/2026-10-01-1255-agentic-ui-roadmap/m-agent-activity-corpus`)
is a growing set of real tasks, from real kinds of people, that the app agent
must be able to do — and a record of what the product cannot do yet. Each
scenario is one small story: who the person is (Sean, Luna, Viatrix, Jordan,
Yona), where things start (a project; a board the app knows, or one plugged
in — blank, running somebody else's firmware, or running LightPlayer), what
they say first, how they answer the agent's questions, which cards they
click, and the checks that decide pass. The list, with Yona's edits, is
`scenarios.md` in that plan; S1–S3 are the original headline evals E1–E3,
under their old file names.

Two stages, so each runs where its tools are:

- **Stage A** — `lpa-studio-core`, `src/app/agent/evals/`. The scenario runs
  in its **seat**, then pure checks judge what it left. Writes
  `target/app-agent-evals/<run>/<scenario>/{project/,transcript.json,report.json}`.
- **Stage B** — `lp-cli/tests/app_agent_emu_decode.rs`. For a C6 scenario
  that names a pin and a strip, the project tree is deployed to the shipped
  `fw-esp32c6` image in the emulator, and the frames on **the scenario's pad**
  (its pin through its board's pin map: D6 = GPIO16, D10 = GPIO18 on the
  XIAO C6) are decoded: whole, error-free, the right LED count, lit, and
  changing over time. Every other scenario reports `stage_b: n/a`.

## The two seats

Neither forks the app chat: both seat the product's own session, tools,
offers and cards on a real `StudioController`, behind one trait
(`ScenarioSeat`, `app_agent_scenario_seat.rs`) and one driver loop
(`drive_scenario`, `app_agent_eval_driver.rs`).

- **The project seat** (`AgentEvalStudio`): a headless Studio with a memory
  library, over an in-process server wearing **the scenario's board's real
  pin map** (`start.board`'s checked-in `boards/<vendor>/<product>.json`), so
  a label the board does not have fails here the way it fails there. The
  board facts the readout cannot show ride the agent's state block as one
  context line, generated from `start.board` (or given verbatim in
  `start.context`).
- **The device seat** (`DeviceScenarioSeat`,
  `src/app/studio/studio_device_e2e_tests/agent_device_seat.rs`): the device
  bench (`DeviceBench`) with a `FakeEsp32Device` behind a scripted USB port,
  on a scripted **or live** model. The board starts the way `start.board.state`
  says: `blank`, `foreign` (`firmware = "factory-demo" | "wled"`; a classic
  ESP32 prints its own ROM banner), `running` (the start golden is installed
  into the library, pushed through the card's own push, and the editor opens
  on the board), or `older` (the same, on older firmware, so Update is
  offered). The fake's host server wears the board's pin map too
  (`FakeDeviceScript::with_board_manifest`). The project a device scenario is
  judged on is **the one the board runs**, read back over the board's own
  wire, and its statuses come from a server wearing the board's pin map.
  Before each message — and before a run a card's click resumes — the seat
  steps the bench until the cards stop moving, as a person waits.

## E4: the device journey (deterministic)

`studio_device_e2e_tests/agent_device_journey_tests.rs` keeps E4's two tests
on the same seat (`AgentSeat`):

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

The corpus re-expresses the journey as S18 on a scripted model
(`app_agent_corpus_tests.rs`). Only two things are fake on the bench: the
model (when scripted) and the board. After the scripted flash the fake
speaks the real wire to a real host `LpServer`, so the push is the real
conversation. **What an emulated C6 would add** (`lp-emu-esp32c6`, as
`just walk-no-board` uses): the shipped `fw-esp32c6` bytes booted through
the ROM and bootloader, the MAC from emulated eFuse, the hello and heartbeat
from real firmware over `lp-link`, and the project compiled by the device's
own JIT and rendered to the LED pin.

## Layout

| Path | What |
|---|---|
| `scenarios/<name>.toml` | One scenario each, S1–S19 (`e1-…`, `e2-…`, `e3-…` keep E1–E3's names). |
| `golden/<name>/` | Hand-built project trees that pass their scenario's project checks. The vendored `modules/*` are catalog `patterns/<slug>/effect/` folders byte for byte. |
| `scripts/<name>.json` | `edit_project` inputs the scripted model replays. |
| `bakeoff.toml` | The bake-off's candidate models. |

## Scenario format

```toml
id = "S18"                      # the corpus id
name = "s18-yona-festival-2am"  # = the file stem
summary = "…"                   # one line for the report
persona = "yona"                # sean | luna | viatrix | jordan | yona
tags = ["device-connect", "flash"]
status = "active"               # active | pending (the live runner skips pending)
waits_for = ""                  # a pending scenario names what it waits for: "M10", "xiao-c6-d4-d5 fix"
note = ""                       # why, in a line
golden = "festival-60-d10"      # the golden that proves the project checks

[start]
project = "none"                # none | "blank" | { golden = "<name>" }
board = { state = "blank", actual = "seeed/xiao-esp32-c6" }
# project seat:  { known = "<board id>" }               the app knows the board
#                { chip = "esp32c6", actual = "<id>" }  the app knows the chip only
#                { actual = "<id>" }                    nothing is known
# device seat:   state = "blank" | "foreign" | "running" | "older",
#                connected = true (port already granted), firmware = "wled"
context = ""                    # replaces the generated context line
note = ""                       # appended to it
selection = ""                  # a node selected in the editor (pending scenarios only, until M7/M8)

[user]
say = "…"                       # the opening message
then = ["…"]                    # follow-ups, after a turn that ends without a question
otherwise = "anything, go."     # the answer to a question nobody scripted (once)
unlisted_cards = "click"        # click | leave: a card no rule names

[[user.reply]]
topic = "board"                 # what `asked` checks name (default: the first keyword)
when = ["board", "which xiao"]  # any of these, at the start of a word in the question
text = "It's a XIAO ESP32-C6."

[[user.card]]
offer = "devices/*/flash"       # a glob over the card's offer path (* within a segment, ** across)
do = "click"                    # click | leave
args = { board = "quinled/dig2go" }   # values the person picks over the agent's

[budget]
turns = 16
usd = 0.40                      # reported cost; the run is stopped past it
tokens = 400_000                # optional: in + out, model-neutral

[[check]]
kind = "output_on"
pin = "D10"
```

**The person's side.** The agent *asked* when its turn's last text has a
`?` in its final 300 characters. Every reply whose keyword starts a word in
that tail is given (joined), each once; if the tail names none, the whole
message is tried. A question nobody scripted gets `otherwise`, once, and is
counted as an *unscripted question* in the report; a second one ends the
run. A turn with no question gets the next `then`; with none left, the
conversation is over. Each card, when it first appears, is clicked or left
by the first matching `[[user.card]]` rule, else by `unlisted_cards`.

## Checks

Project (judged on the saved project, or the board's): `target_is {board}`,
`output_on {pin}`, `strip_of {leds}` (one fixture, lamps in strip order),
`lamp_count {leds}` (any shape), `playlist {min_entries, step_seconds, from}`
(a Cycle playlist; `from` the allowed catalog slugs, any when empty),
`graph_wired`, `all_nodes_ok`, `saved`, `minimal_diff {allow}` (`strip_size`,
`<Kind>.<path>`, `modules`), `unchanged`, `field {node, path, equals |
between, default}`, `entries_removed {patterns}`, `entries_kept {patterns}`,
`entries_added {min, from}`, `any_of {of = [...]}`.

Conversation: `asked {topic}`, `asked_before {topic, before = "pin" | "edit"
| "flash" | "push"}`, `max_questions {n, per_turn}`, `max_turns {n}`,
`card_handed {offer}`, `never {what = "pressed:<glob>" | "act:<glob>" |
"card:<glob>" | "tool:<name>"}` (`pressed` is the agent's own press going
through, as opposed to the person's click on its card), `said_any {words,
last}`, `said_none {words}`.

Board (device seat): `board_runs_project`, `board_firmware {is =
"lightplayer" | "flashed" | "untouched"}`.

E1–E3's ids still parse as kinds: `output_on_d6`, `target_is_xiao_c6`,
`asked_about_board`, `no_d_label_before_board`. Each failure names what it
saw. A golden run skips the conversation and board checks (it had neither).

## Running

```bash
# Deterministic legs (CI): every scenario parses, goldens pass, negatives
# fail, both seats run end to end on a scripted model.
cargo test -p lpa-studio-core app_agent
cargo test -p lpa-studio-core e4_      # the device journey (above)
just test-emu-c6-cli                   # includes stage B on the Sean goldens

# What a live run would do — no model, no key, no money:
just app-agent-corpus --dry-run
just app-agent-eval all --dry-run --include-pending

# Live — costs money, never CI:
just app-agent-corpus                        # every active scenario, GLM-5.3, capped at $2
just app-agent-corpus --max-usd 1 --tag device
just app-agent-corpus --only S4,S7 --include-pending --model <slug>
just app-agent-corpus --repeat 3 --tag pin --against <earlier run id>
just app-agent-eval S18 --model <slug>       # one scenario, no corpus report
just app-agent-bakeoff                       # every bakeoff.toml model × S1–S3 × 3
```

`--max-usd` (default 2) holds the run under a reported spend: each
scenario's own budget is cut to the room left under the cap, and none
starts with less than $0.05 left. The corpus writes `corpus.md` and
`corpus.json` beside the reports: per scenario (pass n/N, turns, tokens,
cost, unscripted questions, cards handed, the first failing check), per tag
and per persona, and the diff against the last corpus run with the same
model (or `--against`). `scripts/app-agent/corpus_report.py <run dir>`
rebuilds them.

The live leg reads the key from `OPENROUTER_API_KEY` or
`~/.lightplayer/settings.json` (`agent.openrouter_api_key`) and never writes
it anywhere. Emulated numbers are `lp-emu:esp32c6:t1` with the `lp-emu`
commit printed beside them; they are not hardware-validated.

## How milestones add scenarios

A pending scenario names its milestone (`waits_for`). That milestone's plan
lists it as an acceptance test, and its PR flips it to `active` — with a
golden when it can, and a recorded live run (`--include-pending --only …`
at the start, for a measured "before"). A milestone adds one or two
scenarios for what it newly makes possible, first as a few lines in
`scenarios.md` style for Yona, then as TOML. A scenario is never weakened to
make a run green.
