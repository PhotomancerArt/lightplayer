# LightPlayer docs style guide

This file governs everything in `docs/user-guide/` — every article a
human or agent writes, edits, or reviews. It is contributor-facing and
deliberately **not** registered in the docs `PAGES` manifest.

## Who we write for

Picture a specific person: a WLED-fluent hobbyist. They have flashed
firmware, picked effects from a list, dragged a speed slider, argued
about palettes on a forum. They are smart, busy, and have **zero
LightPlayer vocabulary**. They are not a beginner at LEDs — they are a
beginner at *our words*, which is a different thing. Write for them and
you write well for everyone on either side of them.

## Voice

Friendly computing. The docs sound like a knowledgeable friend showing
you something they genuinely think is cool — never a manual talking
down to you, never marketing talking past you.

- Second person, present tense. "You drag the knob and the LEDs
  respond."
- Warmth without exclamation-point inflation. One "!" per page is
  plenty; earn it.
- "Isn't this cool?" is the vibe, not a sentence to write.
- Humor is welcome. Snark, never — the reader is never the joke.
- Contractions are fine. Stiffness is not.
- Touchstones: *The Rust Programming Language* (structure, honesty),
  Commodore's "Welcome to the world of friendly computing" (spirit).

## The seven pillars

1. **Show, then tell.** Something moving before prose. On pages with
   live embeds, the hero sim sits above the first paragraph; on static
   pages, lead with a figure or a concrete moment, not a definition.
2. **Name pages after real questions.** "What's a shader?" — not
   "Shader Authoring Concepts". If people keep asking it on calls, that
   phrasing is the title. The page's existence says *great question*,
   never *you should know this*.
3. **Never assume, never talk down.** The first use of any LightPlayer
   term links to the page or glossary entry that explains it. Explain
   the *why* behind the how. No unexplained jargon, no "simply", no
   "just", no "obviously".
4. **Signpost depth honestly.** When a page approaches plumbing the
   reader doesn't need yet, say so, Rust-book style: "You don't need
   this yet — come back when you're wiring multiple boards." Depth is
   an invitation, not a hazing.
5. **Bridge from what readers know.** WLED vocabulary is an on-ramp,
   not a rival: "In WLED you'd pick an effect from the list; here the
   effect is a file you can open." Name the familiar thing, then show
   where ours goes further.
6. **Units read naturally.** 2/s not 0.5s period; 3/min not 0.05/s;
   90 BPM where music is the frame. Numbers appear in the unit a human
   would say out loud.
7. **No problem too small to solve well.** A page about one checkbox
   deserves the same care as the architecture tour. If a thing confuses
   people, it earns a page and a "?" pointing at it.

## Page mechanics

- **Markdown subset:** CommonMark + strikethrough. No tables, no raw
  HTML (it renders escaped, on purpose). Images per the docs renderer's
  current support; every image gets real alt text.
- **Live embeds** use the `embed` directive fence:

  ~~~
  ```embed panel sim=disc mode=interactive
  ```
  ~~~

  Embed names, sim names, and docs links are validated at build time —
  a typo is a failing check, not a broken page.
- **Links:** in-docs links are `#/docs/<slug>` (optionally
  `#/docs/<slug>#<anchor>`), and they are checked. External links are
  plain `https://`.
- **Endings:** every page ends with a short "Where next" — one or two
  links continuing the reader's most likely path.
- **Length:** a page is one question answered well, readable in about
  five minutes. When it grows past that, it is two pages.
- **Headings** are sentence case ("The reveal", not "The Reveal") and
  become anchor targets — keep them stable once a "?" links to them.

## Terminology canon

Use these words, exactly, everywhere. First use on a page links to the
explaining page once it exists.

- **shader** — the friendly little program that computes colors. Never
  "effect" for our own thing (WLED's effects are "effects"; the bridge
  sentence may use both).
- **product** — what a node produces (visual, control, time…).
- **module** — a package of nodes that works as one thing.
- **node** — one unit inside a module (a clock, a fixture, a shader).
- **mapping** — where each LED physically sits; the bridge from pixels
  to space.
- **sim** — the in-browser simulated board. "Simulator" on first use,
  "sim" after.
- **device / board** — real hardware. "Device" in UI-adjacent prose,
  "board" when hands are on hardware.
- **bound / binding** — a slot wired to a bus or channel. Bound things
  render **violet** in Studio; teach the color once per page at most.
- **knob** — friendly word for a panel control in docs prose (the UI
  may say "control").

**Unsettled — do not use in prose until ratified (G1):**

- *phasor* vs *LFO*: "phasor" is the product's word; whether docs
  explain it via "LFO" as the familiar bridge term is an open ruling
  (Q5).
- Tagline: lean "Friendly shaders, in your pocket" (Q1); final wording
  belongs to the landing-page initiative. Docs don't print taglines
  meanwhile.

## For agents specifically

- Read this file before writing or editing any article; hold drafts to
  it ruthlessly. Voice drift is a defect.
- Prose must be original — never adapted from GPL or other
  incompatible sources (see `AGENTS.md`, license discipline).
- When an article needs a mechanism that doesn't exist (an embed kind,
  a renderer feature), stop and flag it; never fake it in prose.
- Check the rendered page, not just the markdown — the docs renderer
  supports less than GitHub does.
