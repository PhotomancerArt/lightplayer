---
name: session-titles
description: Rename Claude sessions to Yona's format ("Jul-15-1030 - live previews") — a janitor sweep over OTHER sessions, since a session cannot rename itself. Use when asked to fix chat names, tidy the session list, or retitle a specific session.
---

# Session titles

Yona names chats `Mon-DD-HHMM - short topic`. The harness auto-titles new
sessions as prose ("Studio UI performance probe"), so the list drifts out of
format and needs periodic sweeping.

## The hard constraint

**A session cannot rename itself.** `set_session_title` requires a
`session_id` that is not the current session, and the current session is
invisible to `list_sessions` / `get_session` (both return "Session not
found" for it). This is why titling can never be a self-service step in some
other workflow — it only works as a sweep over *other* sessions.

Corollary: this session's own title cannot be fixed from here. If it needs
renaming, it gets picked up by the next sweep run from a different session.

## Format

```
Mon-DD-HHMM - short topic
```

- **Timestamp** — the session's `createdAt` (session start, not last
  activity), converted from the stored UTC to **local time**. Converting
  matters: a session created `2026-07-23T06:14:02Z` is `Jul-22-2314` local,
  a different calendar day.
- **Topic** — 2–4 words, **lowercase**, no trailing punctuation. Real
  examples: `live previews`, `device ux`, `shader fuel`, `multi dev servers`,
  `binding`.
- **Follow-ups on the same topic** take a numeric suffix: `device ux 2`.
  This marks a second session about the same work — it is not a
  disambiguator for two sessions that merely share a timestamp (different
  topics already read differently).

## Sweep

1. `list_sessions` (the current session is excluded automatically).
2. Skip titles already matching `^[A-Z][a-z]{2}-\d{2}-\d{4} - `.
3. For each remaining session, `get_session` → `createdAt`.
4. Convert UTC → local:

```bash
epoch=$(TZ=UTC date -j -f "%Y-%m-%dT%H:%M:%SZ" "2026-07-27T15:32:58Z" +%s)
date -r "$epoch" "+%b-%d-%H%M"   # -> Jul-27-0832
```

   Always compute this; do not convert by hand.
5. Derive the topic. The existing auto-title is usually a good source —
   shorten and lowercase it (`"Studio UI performance probe"` →
   `studio perf probe`; `"Fix unreachable_patterns warning in
   device_controller test helper"` → `unreachable pattern fix`). When the
   auto-title is uninformative, read the session's first user message with
   `list_events` instead.
6. `set_session_title` with the new name.
7. Report the renames as a table (old → new) so the mapping is reviewable.

Sweeping running sessions is safe — the title is cosmetic and does not
disturb the work in them.

## Notes

This skill is harness-general rather than LightPlayer-specific; it lives in
this repo because this repo is the version-controlled home for Yona's agent
process files (`~/.claude/` and the Dropbox planning workspace are not git
repos). Copy it to `~/.claude/skills/` if sessions in other repos need it.
