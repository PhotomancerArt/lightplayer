---
status: fixed
found: 2026-08-24 # how: report (P6 patch-panel story captures)
fixed: this change
area: lpa-studio-web (every rsx select)
class: assumed-context
related: []
---

# A select's bound `value:` lands before its options mount, so first render shows the first option

**Symptom** — In the P6 patch-panel story captures
(`studio/patch/patch-panel/patch-panel-derived` / `patch-panel-armed`,
branch `claude/hungry-noyce-b6822b`), the OutputPane's port picker reads
"1 · IO18 · port 0 · full" — the first option — while the real selection
(and the section header) is "Box 2 · IO14". Any rsx `select` that binds
only `value:` shows its first option on first render regardless of the
bound value; interaction still dispatches correctly, and later re-renders
that happen to rewrite `value` self-heal, which is why the defect hides
outside stories and cold loads.

**Root cause** — Mounting a Dioxus template applies an element's dynamic
attributes before its dynamic children exist. The web interpreter
(`dioxus-interpreter-js` `set_attribute.js`) writes a select's `value` as
the DOM property (`node.value = …`); with no `<option>` children attached
yet the assignment matches nothing and degrades to `""`, and when the
options are appended a moment later the browser's selectedness reset
falls back to displaying the first option. The idiom presumes the DOM
select can hold a value independent of its options — it cannot; the
options are the source of truth for selectedness.

**Fix** — Mirror the bound value onto each option:
`selected: option_value == bound_value` (the interpreter writes
`option.selected` as a property, which the select honors when the option
is inserted). Keep the select-level `value:` for re-render sync. Five of
six Studio sites already carried the mirror; this change adds it to the
sixth (`app/home/package_card.rs`, the new-from-pattern export picker)
and pins the idiom app-wide.

**Regression coverage** —
`select_mirror_lint::every_value_bound_select_mirrors_selected_onto_its_options`
(host test) walks the crate's rsx sources and fails any `select` that
binds `value:` without a `selected:` mirror — this catches *new* sites,
including branches in flight, at CI time. Visually, the
`studio/node/slot-value-editor/dropdown-field-wired` story renders a real
`<select>` pinned to a non-first option; a regression flips its capture
from "Blast" to "Idle".

**Lesson** — A framework that clones templates and hydrates them applies
attributes and children in an order the author never sees, so any DOM
property whose meaning depends on *other* nodes (a select's `value`, but
also e.g. `selectedIndex`, or anything resolved against children) cannot
be trusted as a write-once mount attribute. State it on the node that
owns it — selectedness belongs to the option, not the select. When an
idiom is repeated per-site, guard it with a source lint, not review
memory: the sixth copy of the idiom (and the seventh, on a branch) is
exactly where the mirror went missing.
