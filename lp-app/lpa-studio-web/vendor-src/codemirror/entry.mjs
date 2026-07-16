// LightPlayer Studio's CodeMirror 6 façade.
//
// Bundled by esbuild (see package.json) into an IIFE that assigns
// `globalThis.LpCodeMirror`, committed at
// lp-app/lpa-studio-web/public/vendor/codemirror/codemirror.js and loaded by
// a plain <script defer> tag in index.html. The Rust side
// (src/base/code_editor.rs) talks only to the API exported here — keep this
// surface minimal and stable so bundle regenerations stay rare.

import { autocompletion, snippetCompletion } from "@codemirror/autocomplete";
import { Compartment, EditorState } from "@codemirror/state";
import {
  EditorView,
  drawSelection,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
} from "@codemirror/view";
import {
  defaultKeymap,
  history,
  historyKeymap,
  indentWithTab,
} from "@codemirror/commands";
import {
  HighlightStyle,
  StreamLanguage,
  bracketMatching,
  indentUnit,
  syntaxHighlighting,
} from "@codemirror/language";
import { clike } from "@codemirror/legacy-modes/mode/clike";
import { xml } from "@codemirror/legacy-modes/mode/xml";
import { lintGutter, setDiagnostics } from "@codemirror/lint";
import { tags } from "@lezer/highlight";

function words(list) {
  const set = {};
  for (const word of list.split(" ")) set[word] = true;
  return set;
}

// GLSL via the legacy clike factory: deterministic and tiny, versus pulling
// in a full grammar. Keyword sets follow GLSL ES 3.0 (the shader dialect the
// LightPlayer frontend accepts).
const glslLanguage = StreamLanguage.define(
  clike({
    name: "glsl",
    keywords: words(
      "const uniform buffer shared attribute varying coherent volatile restrict " +
        "readonly writeonly layout centroid flat smooth noperspective patch sample " +
        "break continue do for while switch case default if else in out inout " +
        "invariant precise discard return struct precision highp mediump lowp",
    ),
    types: words(
      "void bool int uint float double " +
        "vec2 vec3 vec4 dvec2 dvec3 dvec4 bvec2 bvec3 bvec4 ivec2 ivec3 ivec4 " +
        "uvec2 uvec3 uvec4 mat2 mat3 mat4 mat2x2 mat3x3 mat4x4 " +
        "sampler1D sampler2D sampler3D samplerCube sampler2DShadow",
    ),
    builtin: words(
      "radians degrees sin cos tan asin acos atan sinh cosh tanh " +
        "pow exp log exp2 log2 sqrt inversesqrt abs sign floor trunc round " +
        "ceil fract mod modf min max clamp mix step smoothstep length distance " +
        "dot cross normalize faceforward reflect refract matrixCompMult " +
        "transpose determinant inverse texture texelFetch textureSize " +
        "dFdx dFdy fwidth",
    ),
    atoms: words("true false gl_FragCoord gl_FragColor gl_Position gl_VertexID"),
  }),
);

const xmlLanguage = StreamLanguage.define(xml);

// Chrome pulled from the studio CSS custom properties (src/style.css) with
// literal fallbacks matching the committed dark palette, so the editor works
// even before the app stylesheet loads.
const studioTheme = EditorView.theme(
  {
    "&": {
      backgroundColor: "var(--studio-color-terminal, #0c1114)",
      color: "var(--studio-color-text, #f2f0e8)",
      fontSize: "12px",
      height: "100%",
    },
    ".cm-scroller": {
      fontFamily:
        "var(--studio-font-mono, SFMono-Regular, Consolas, Menlo, monospace)",
      lineHeight: "1.5",
    },
    ".cm-content": {
      caretColor: "var(--studio-color-accent, #7be0b2)",
    },
    ".cm-cursor, .cm-dropCursor": {
      borderLeftColor: "var(--studio-color-accent, #7be0b2)",
    },
    "&.cm-focused": { outline: "none" },
    ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": {
      backgroundColor: "rgba(123, 224, 178, 0.16)",
    },
    ".cm-gutters": {
      backgroundColor: "var(--studio-color-surface-muted, #11161b)",
      color: "var(--studio-color-text-subtle, #99a2ad)",
      border: "none",
      borderRight: "1px solid var(--studio-color-border-muted, #252d34)",
    },
    ".cm-activeLine": {
      backgroundColor: "var(--studio-color-bg-wash, rgba(255, 255, 255, 0.04))",
    },
    ".cm-activeLineGutter": {
      backgroundColor: "var(--studio-color-bg-wash, rgba(255, 255, 255, 0.04))",
    },
    ".cm-lineNumbers .cm-gutterElement": { minWidth: "2.4em" },
    // A visible error marker in the margin next to the offending line —
    // drop CodeMirror's default icon and draw a studio-toned red dot.
    ".cm-lint-marker-error": { content: "none" },
    ".cm-lint-marker-error::after": {
      content: '""',
      display: "block",
      boxSizing: "border-box",
      width: "0.72em",
      height: "0.72em",
      margin: "0.14em auto 0",
      borderRadius: "50%",
      backgroundColor: "var(--studio-status-error-foreground, #f08a8a)",
    },
  },
  { dark: true },
);

const studioHighlight = HighlightStyle.define([
  { tag: tags.keyword, color: "var(--studio-color-heading, #94b8aa)" },
  { tag: tags.typeName, color: "var(--studio-color-accent, #7be0b2)" },
  { tag: [tags.number, tags.atom], color: "#e0c37b" },
  { tag: tags.string, color: "#e09a7b" },
  {
    tag: tags.comment,
    color: "var(--studio-color-text-subtle, #99a2ad)",
    fontStyle: "italic",
  },
  { tag: tags.meta, color: "var(--studio-color-text-muted, #c7cbd0)" },
  {
    tag: [tags.operator, tags.punctuation],
    color: "var(--studio-color-text-muted, #c7cbd0)",
  },
  { tag: tags.variableName, color: "var(--studio-color-text, #f2f0e8)" },
]);

function languageExtension(name) {
  switch (name) {
    case "glsl":
      return glslLanguage;
    case "xml":
      return xmlLanguage;
    default:
      return [];
  }
}

// Convert one façade diagnostic ({line, col, message, severity?}; line/col
// 1-based) into a CodeMirror lint diagnostic, clamped into the current doc.
function toDiagnostic(state, entry) {
  const lineCount = state.doc.lines;
  const lineNumber = Math.min(Math.max(entry.line ?? 1, 1), lineCount);
  const line = state.doc.line(lineNumber);
  const col = Math.max((entry.col ?? 1) - 1, 0);
  const pos = Math.min(line.from + col, line.to);
  // Widen the underline from a single char to the whole token at the error
  // position so it is actually visible; fall back to one char at
  // punctuation/whitespace where there is no word.
  const word = state.wordAt(pos);
  const from = word ? word.from : pos;
  const to = word ? word.to : Math.min(pos + 1, line.to);
  return {
    from,
    to: Math.max(to, from),
    severity: entry.severity ?? "error",
    message: entry.message ?? "",
  };
}

// Convert one façade completion entry into a CodeMirror `Completion`.
// Entries are plain objects: { label, detail?, type?, info?, snippet? }.
// A `snippet` (CodeMirror `snippet()` template, e.g. "mix(${x}, ${y}, ${a})")
// makes accepting insert the template with navigable placeholders; without
// one, accepting inserts the label verbatim.
function toCompletion(entry) {
  const base = {
    label: entry.label ?? "",
    detail: entry.detail || undefined,
    type: entry.type || undefined,
    info: entry.info || undefined,
  };
  if (entry.snippet) return snippetCompletion(entry.snippet, base);
  return base;
}

// The autocompletion extension for a completion list; empty list = no
// extension at all, so editors without completions (plain text, XML) never
// grow a popup. The source matches on word prefixes and also fires on
// explicit request (Ctrl-Space) at any position.
function completionExtension(entries) {
  if (!entries.length) return [];
  const options = entries.map(toCompletion);
  const source = (context) => {
    const word = context.matchBefore(/\w+/);
    if (!word && !context.explicit) return null;
    return {
      from: word ? word.from : context.pos,
      options,
      validFor: /^\w*$/,
    };
  };
  return autocompletion({ override: [source] });
}

// Create an editor under `parent`. Options:
//   doc              initial text (also the initial clean baseline)
//   language         "glsl" | "xml" | anything-else = plain
//   readOnly         boolean
//   completions      optional array of { label, detail?, type?, info?,
//                    snippet? } (see toCompletion); empty/absent = no popup
//   onModified(bool) fired when the modified-vs-baseline state flips
//   onChange(text)   fired with the full text after every document change
//                    (user typing and external setDoc alike)
//   onApplyRequested() fired on Mod-Enter
//   onSaveRequested()  fired on Mod-s (always swallowed, so the browser's
//                      save dialog never opens while the editor is focused)
// Returns the imperative handle the Rust component drives.
export function createEditor(parent, opts = {}) {
  const readOnly = new Compartment();
  const completions = new Compartment();
  let baseline = opts.doc ?? "";
  let modified = false;

  const notifyModified = (next) => {
    if (next === modified) return;
    modified = next;
    opts.onModified?.(modified);
  };

  const updateListener = EditorView.updateListener.of((update) => {
    if (!update.docChanged) return;
    const text = update.state.doc.toString();
    opts.onChange?.(text);
    notifyModified(text !== baseline);
  });

  // Listed first so it out-prioritizes defaultKeymap's own Mod-Enter. The
  // `run → true` returns also stop the browser: Mod-s in particular must
  // never fall through to the native save dialog while the editor is
  // focused, even when the app treats the save request as a no-op.
  const editorKeymap = keymap.of([
    {
      key: "Mod-Enter",
      run: () => {
        opts.onApplyRequested?.();
        return true;
      },
    },
    {
      key: "Mod-s",
      run: () => {
        opts.onSaveRequested?.();
        return true;
      },
    },
  ]);

  const state = EditorState.create({
    doc: baseline,
    extensions: [
      editorKeymap,
      lineNumbers(),
      highlightActiveLineGutter(),
      highlightSpecialChars(),
      history(),
      drawSelection(),
      bracketMatching(),
      highlightActiveLine(),
      lintGutter(),
      studioTheme,
      syntaxHighlighting(studioHighlight),
      languageExtension(opts.language),
      indentUnit.of("    "),
      readOnly.of(EditorState.readOnly.of(Boolean(opts.readOnly))),
      completions.of(
        completionExtension(
          Array.isArray(opts.completions) ? opts.completions : [],
        ),
      ),
      keymap.of([indentWithTab, ...defaultKeymap, ...historyKeymap]),
      updateListener,
    ],
  });

  const view = new EditorView({ state, parent });

  return {
    getDoc: () => view.state.doc.toString(),
    // External resync: replaces the whole doc AND the clean baseline. Keeps
    // undo history (replacing text is itself undoable) — acceptable because
    // resyncs only land when the editor is unmodified.
    setDoc: (text) => {
      baseline = text;
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: text },
      });
      notifyModified(false);
    },
    // The current text becomes the clean baseline (e.g. right after Apply).
    markClean: () => {
      baseline = view.state.doc.toString();
      notifyModified(false);
    },
    isModified: () => modified,
    setReadOnly: (value) => {
      view.dispatch({
        effects: readOnly.reconfigure(EditorState.readOnly.of(Boolean(value))),
      });
    },
    setCompletions: (list) => {
      view.dispatch({
        effects: completions.reconfigure(
          completionExtension(Array.isArray(list) ? list : []),
        ),
      });
    },
    setDiagnostics: (list) => {
      const mapped = (Array.isArray(list) ? list : []).map((entry) =>
        toDiagnostic(view.state, entry),
      );
      view.dispatch(setDiagnostics(view.state, mapped));
    },
    clearDiagnostics: () => {
      view.dispatch(setDiagnostics(view.state, []));
    },
    revealLine: (lineNumber) => {
      const clamped = Math.min(Math.max(lineNumber, 1), view.state.doc.lines);
      const line = view.state.doc.line(clamped);
      view.dispatch({
        selection: { anchor: line.from },
        effects: EditorView.scrollIntoView(line.from, { y: "center" }),
      });
    },
    focus: () => view.focus(),
    destroy: () => view.destroy(),
  };
}
