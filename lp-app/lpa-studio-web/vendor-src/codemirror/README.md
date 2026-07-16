# CodeMirror 6 vendor bundle

This directory builds the vendored CodeMirror 6 bundle committed at
`lp-app/lpa-studio-web/public/vendor/codemirror/codemirror.js`. The studio
app loads that file with a plain `<script defer>` tag (see `index.html`) and
talks to it through `globalThis.LpCodeMirror` — the façade defined in
[`entry.mjs`](entry.mjs), consumed by `src/base/code_editor.rs`.

**npm is needed only to regenerate the bundle.** Building and running the
studio app never touches npm; the committed bundle is the artifact (same
philosophy as the pre-generated `assets/tailwind.css`).

## Regenerating

```bash
just studio-codemirror-bundle
# equivalent to: cd here && npm ci && npm run build
```

Commit the regenerated `public/vendor/codemirror/codemirror.js` together
with whatever `entry.mjs` / dependency change motivated it.

## What's in the bundle

Pinned in `package.json` / `package-lock.json`:

- `@codemirror/state`, `@codemirror/view`, `@codemirror/commands`,
  `@codemirror/language` — the editor core (history, default keymap,
  line numbers, selection drawing, bracket matching).
- `@codemirror/legacy-modes` — GLSL highlighting via the `clike` factory
  (keyword sets in `entry.mjs`; there is no official CM6 GLSL grammar) and
  `xml` for SVG sources.
- `@codemirror/lint` — diagnostics gutter/underline machinery behind
  `setDiagnostics` on the façade handle.
- `@codemirror/autocomplete` — the completion popup behind the
  `completions` option / `setCompletions` handle method (entries carry
  label, signature detail, optional snippet + info; empty list = the
  extension is absent, so plain/XML editors never grow a popup).
- `@lezer/highlight` — tags for the studio-token highlight style.

Intentionally excluded: search panel, folding, multiple themes — keep the
bundle lean; add a package only when a feature needs it, and record the
size change (`npm run build` prints it; currently ~375 KB minified —
autocomplete added ~22 KB).

## Façade contract

`createEditor(parent, {doc, language, readOnly, completions, onModified,
onChange, onApplyRequested, onSaveRequested})` returns a handle with
`getDoc` / `setDoc` / `markClean` / `isModified` / `setReadOnly` /
`setDiagnostics` / `clearDiagnostics` / `setCompletions` / `revealLine` /
`focus` / `destroy`. Semantics are
documented in `entry.mjs`; the Rust component's docs describe the ownership
rules. Mod-Enter (apply) and Mod-s (save) are handled inside the editor's
own keymap (highest precedence) and call `onApplyRequested` /
`onSaveRequested`; both always consume the keystroke, so Mod-s never
reaches the browser's save dialog while the editor is focused.
