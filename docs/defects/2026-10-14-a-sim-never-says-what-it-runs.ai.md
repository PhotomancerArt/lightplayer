---
status: fixed
found: 2026-10-10      # how: the connected feel gate's preview (a simulated S3 Plus added as a second board); finding 2
fixed: this change
area: lpa-devices `IdentifyActivity` × `fw-browser` (no heartbeat) × lpa-studio-core's bench sim fake
class: stand-in-divergence
related:
  - lp2025/2026-10-08-2330-connected-in-the-card (the connected feel gate, findings 2 and 15)
---
# A sim never says what it runs

**Symptom** — a running simulated board with nothing on it read "Not known
yet" on its project bar, with no Add a project, and its Connect stayed
disabled ("Nothing on it yet"). Nothing could be put on it from its card.

**Root cause** — what a board runs reaches the card from its own report:
firmware volunteers it in every heartbeat, and the device model asks
(`ListLoadedProjects`) only after a push, a removal or Clear faults. The
browser sim (`fw-browser`, through `fw_core::tick_server_frame`) sends no
heartbeat at all, so a fresh sim never said, and the card waited forever.
Studio's own tests did not see it: the bench's sim fake heartbeats every
20 ms ("exactly as fw-browser will"), which the real sim never did.

**Fix** — the identify asks once, as it ends, when a LightPlayer has not
said what it runs (`IdentifyActivity::ask_what_it_runs`). The answer folds
like a heartbeat's report. On firmware it is usually already said and
nothing is sent.

**Regression coverage** —
`identify::tests::a_light_player_that_has_not_said_what_it_runs_is_asked_as_identify_ends`,
and `board_card_tests::a_running_sim_with_no_project_offers_add_a_project`,
whose sim fake sends no heartbeat (it fails without the ask).

**Lesson** — a test double that is more talkative than the thing it stands
in for hides every path that depends on the silence. The bench's sim fake
still heartbeats elsewhere; new sim tests should start from a silent one.
