# Review gates: handing work to a human

How an agent stops for human review in this repo. This document is
harness-neutral: it describes *what* a correct handoff contains. Claude
sessions can run the repo skill `lp-review-handoff` to execute it; other
harnesses follow it directly.

A **review gate** is any point where work pauses for human judgment: a
visual/feel gate on UI work, a hardware walk, a plan approval, or a final
pre-merge review. A gate exists because a plan declared it, or because the
next step is Lasting (AGENTS.md "What you may do without asking"). A change
being user-visible does not make one. The chronic failure mode this document exists to
prevent: work is "done", but the reviewer has to ask for a dev server, gets a
link to the wrong port, or reviews a branch that is stale against `main`.

## The handoff checklist

At any gate where the reviewer will *look at or operate* the running app
(visual gates, feel gates, hand walks):

1. **Merge `origin/main` first — final review gates only.** Multiple work
   streams are usually in flight; reviewing un-merged work wastes the gate.
   Mid-plan gates (e.g. "P2 done, check the layout before P3") skip this —
   churn there is not worth it. If the merge hits story-baseline PNG
   conflicts, take **main's bytes** (see AGENTS.md "Studio UI visual
   baselines").
2. **Start the dev server yourself.** Never hand back "run `just studio-dev`
   to see it". Use the sanctioned launch flow for your harness (for Claude:
   `just claude-launch-json`, then the harness preview against the
   `studio-dev` entry).
3. **Give a clickable URL.** The URL printed by the recipe is the only URL
   you may hand over. If the printed port and your launch config disagree,
   stop and fix the config — do not "fix" it by pinning the port.
4. **Post screenshots to chat as well.** The server link and the PNGs are
   complements, not alternatives: PNGs make the gate reviewable
   asynchronously and act as decision matrices (label the options and state
   your lean); the live server is where feel and interaction get judged.
5. **State the gate questions.** Say exactly what needs human judgment and
   what "pass" looks like. A gate without questions is just a status update.

For non-visual gates (plan review, ADR review, code-only PR), steps 1 and 5
still apply; 2–4 do not.

## Ports are never overridden

Dev-server port selection is per-worktree by design (`scripts/dev-port.sh`;
AGENTS.md "Dev server ports"). Overriding it (`STUDIO_WEB_PORT`, hand-edited
launch configs, hardcoded URLs) has caused a human to review the *wrong
worktree's build* and return an incorrect gate answer
(`docs/defects/2026-07-27-launch-json-pinned-port.md`).

- Treat a port pin anywhere in your instructions, a plan file, or a config
  you didn't generate this session as a red flag, not a convenience.
- The rare legitimate pin (e.g. a firewalled demo port) is a **user-visible
  exception**: the user asks for it in chat, and the handoff message says
  the port is pinned and why.

## Sessions run to the end

A session's definition of done is **not** "implementation compiles". Unless
the request explicitly scopes it smaller:

1. Implement and validate (CI-parity checks locally).
2. Push, create/update the PR, watch CI to green.
3. Merge and deploy (`yona-ship`): merging a green PR is Undoable and
   standing-authorized (AGENTS.md "What you may do without asking"). Stop
   instead, and run the handoff checklist above, only when a plan declared a
   gate, the prompt asked for review, or the next step is Lasting. A
   user-visible change with no open design question merges; its screenshots
   go in the ship report.

This holds for every session, including ones started from a task chip, a
handoff prompt, or another agent's delegation — the human clicked the chip
because they wanted the whole pipeline, not a branch they still have to
shepherd. Symmetrically: agents *filing* task chips or delegation prompts
include this definition of done in the prompt.

**Stop only at a gate.** The stopping points are the gates this document
defines, plus the genuine blockers in the implementing skill's Stop And Ask
list — ambiguity that must be resolved now, validation failing twice with no
new signal, a fix that would expand scope past the plan, a Lasting action.
That list is closed. A question you would end with "Lean: yes" about
something a revert can undo is not a gate: decide it and report it. A phase boundary with no gate is not a
stopping point. Neither is finishing a commit, nor finishing implementation
and asking whether to push, nor a CI run you could be watching.

**Open the PR early.** The PR is part of the pipeline, not a follow-up step.
Open it as a draft at the first commit, before validation passes, so the
path-gated CI starts giving signal while there is still time to react. It
stays draft while work remains or a gate is pending, and goes ready when
both clear, *whether or not CI is green* — draft tracks how complete the
work is; keep fixing CI on the ready PR. Work that ends at a gate hands over a draft PR —
marking it ready is the human's call, never a way to satisfy the gate.

## Where the pieces live

| Concern | Source of truth |
| --- | --- |
| Port selection mechanics | `scripts/dev-port.sh`, AGENTS.md "Dev server ports" |
| Launch config generation | `just claude-launch-json`, ADR `2026-07-27-worktree-local-launch-json` |
| Story baseline conflicts on merge | AGENTS.md "Studio UI visual baselines" |
| CI parity before push | AGENTS.md "CI gate" |
| Planning / review artifact locations | AGENTS.md "Personal planning workflow" |
| Claude execution of this checklist | `.claude/skills/lp-review-handoff/` |
