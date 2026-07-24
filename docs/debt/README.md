# Debt register

Standing burdens we consciously carry. The third register: ADRs record
decisions, defects record failures, debt records **conditions** — a
weak subsystem, a structural tax, a workaround-encrusted area that we
have chosen (for now) to live with. Naming the burden makes carrying
it intentional instead of frustrating.

Entries are **named by slug, not date** — `story-capture-pipeline.md`
— because a debt entry is a long-lived handle cited from defects,
chips, and plans; dates live in frontmatter (`since`/`logged`) and in
the incident log. (Defects and ADRs stay date-named: they are events;
debt is a condition.)

## The filing bar

File debt when a burden is **structural and recurring** — it taxes
work repeatedly, has resisted (or not merited) an immediate fix, and
somebody keeps re-learning its workarounds. One entry per burden, not
per incident: incidents APPEND to the entry's log. Todos, feature
ideas, and one-off deferrals do not belong here — they stay task
chips and planning notes.

## Entry template

```markdown
---
status: carried        # carried | paying-down | retired
since: YYYY-MM-DD      # best-effort inception of the condition
logged: YYYY-MM-DD     # when this entry was filed
area: <subsystem>
related: []            # defects, ADRs, chips, plan dirs
---
# <the burden, named>

**Shape** — what is weak and why it is structural, not one bug.
**Carrying cost** — what it taxes, concretely (time, flakes, blocked
gates, re-learned lore).
**Workarounds** — the operational knowledge that makes it livable
(exact incantations; keep current).
**Incident log** — dated, append-only. The accumulating evidence; a
lengthening log is the paydown-priority signal.
**Exit criteria** — what "paid down" observably means. Debt without an
exit definition is a complaints file.
```

Paying down debt is often a real decision among alternatives (rebuild
vs replace vs relocate) — when it is, the decision becomes an ADR and
the entry links it, flips to `paying-down`, then `retired` (entries
stay in place when retired; the log is the history).

## Index

| Entry | Status | Since | Area | Cost in one line |
| --- | --- | --- | --- | --- |
| [story-capture-pipeline](story-capture-pipeline.md) | carried | 2026-07-08 | studio-web/story-capture | ~15 min + flake retries per UI change; visual gates block under load |
| [web-serial-js-untestable](web-serial-js-untestable.md) | carried | 2026-07-10 | lpa-link/browser-serial | JS session/flash layer ships untested; bugs surface only on hardware |
