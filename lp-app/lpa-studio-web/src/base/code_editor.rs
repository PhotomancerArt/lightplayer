//! CodeMirror-backed code editor leaf component.
//!
//! Wraps the vendored CodeMirror 6 bundle (`public/vendor/codemirror/`,
//! loaded by `index.html` as `globalThis.LpCodeMirror`; regenerate with
//! `just studio-codemirror-bundle`). This is the app's first third-party JS
//! widget, so the ownership rules are strict:
//!
//! - **The component owns its DOM subtree.** Dioxus renders one stable
//!   container `div`; CodeMirror is mounted into it imperatively in
//!   `onmounted` and torn down in `use_drop`. Nothing inside the container
//!   is ever diffed by Dioxus.
//! - **`doc` is the external truth** (e.g. the effective asset content).
//!   Reconciliation rule: when `doc` changes, it replaces the editor text
//!   only while the editor is *unmodified*; if the new `doc` exactly equals
//!   the current editor text the editor is just marked clean (this is how an
//!   applied edit's ack clears the modified state); otherwise the user's
//!   unsaved text wins and the modified chrome stays on.
//! - **Callbacks are decoupled from the JS stack.** The bundle's callbacks
//!   write signals; `use_effect` forwards them to the `on_modified` /
//!   `on_apply` handlers inside the Dioxus runtime.
//! - **Stories:** the container carries `data-story-wait="1"` until the
//!   editor is initialized and laid out, so PNG capture waits for it.
//!
//! `language` is fixed at mount; changing it on a live editor is not
//! supported (remount with a different `key` instead).

use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};

static NEXT_CODE_EDITOR_ID: AtomicUsize = AtomicUsize::new(1);

/// How long to keep polling for the vendored bundle before giving up.
const BUNDLE_POLL_INTERVAL_MS: u32 = 50;
const BUNDLE_POLL_LIMIT: u32 = 100;

/// Syntax mode for [`CodeEditor`], mapped onto the vendored bundle's
/// language identifiers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodeEditorLanguage {
    Glsl,
    Xml,
    Plain,
}

impl CodeEditorLanguage {
    fn as_str(self) -> &'static str {
        match self {
            Self::Glsl => "glsl",
            Self::Xml => "xml",
            Self::Plain => "plain",
        }
    }
}

/// One editor diagnostic (1-based line/col), rendered as a lint underline
/// plus gutter marker at that position.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeEditorDiagnostic {
    pub line: u32,
    pub col: u32,
    pub message: String,
}

/// CodeMirror-backed editor. See the module docs for the ownership and
/// `doc`-reconciliation rules.
///
/// - `on_modified` fires when the modified-vs-`doc` state flips.
/// - `on_change` fires with the full editor text after every document
///   change (typing and external resyncs alike), so a parent can mirror the
///   current text — e.g. into the payload of an apply control rendered
///   outside the editor — without polling. Stays app-agnostic: the editor
///   neither knows nor cares what the mirror is for.
/// - `on_apply` fires on Cmd/Ctrl+Enter with the current editor text; the
///   parent decides what "apply" means (the editor never marks itself clean
///   on apply — the ack comes back through the `doc` prop).
/// - `on_save` fires on Cmd/Ctrl+S while the editor is focused; the parent
///   decides what "save" means (a no-op is fine). The editor always swallows
///   the keystroke, so the browser's save dialog never opens.
/// - `reveal_line` scrolls to and selects a 1-based line whenever its value
///   changes to `Some(new)` (epoch-style: repeat reveals of the same line
///   need a `None` in between).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn CodeEditor(
    doc: String,
    #[props(default = CodeEditorLanguage::Plain)] language: CodeEditorLanguage,
    #[props(default = false)] read_only: bool,
    #[props(default = Vec::new())] diagnostics: Vec<CodeEditorDiagnostic>,
    #[props(default = None)] reveal_line: Option<u32>,
    #[props(default = None)] on_modified: Option<EventHandler<bool>>,
    #[props(default = None)] on_change: Option<EventHandler<String>>,
    #[props(default = None)] on_apply: Option<EventHandler<String>>,
    #[props(default = None)] on_save: Option<EventHandler<()>>,
    #[props(default = String::new())] class: String,
) -> Element {
    let container_id = use_hook(|| {
        let id = NEXT_CODE_EDITOR_ID.fetch_add(1, Ordering::Relaxed);
        format!("ux-code-editor-{id}")
    });
    // The live editor handle plus the desired props as of the latest render.
    // The init task and the JS callbacks share it; render reconciles through
    // it without any signal writes (so no render-loop hazards).
    let shared = use_hook(|| {
        Rc::new(RefCell::new(EditorShared {
            editor: None,
            desired: DesiredState {
                doc: String::new(),
                read_only: false,
                diagnostics: Vec::new(),
                reveal_line: None,
                language: CodeEditorLanguage::Plain,
            },
        }))
    });
    let init_state = use_signal(|| CodeEditorInitState::Loading);
    // Mirrors written by the JS callbacks, forwarded to the props by effects.
    let modified_mirror = use_signal(|| false);
    let apply_request = use_signal(|| None::<ApplyRequest>);
    let save_request = use_signal(|| None::<SaveRequest>);
    let change_notice = use_signal(|| None::<ChangeNotice>);

    // Keep the shared desired-state current with this render's props, and
    // reconcile the live editor against it (no-ops when nothing changed).
    {
        let mut shared_mut = shared.borrow_mut();
        shared_mut.desired = DesiredState {
            doc: doc.clone(),
            read_only,
            diagnostics: diagnostics.clone(),
            reveal_line,
            language,
        };
        shared_mut.reconcile();
    }

    // Forward the modified mirror to the prop handler inside the runtime.
    let modified_forward = use_hook(|| Rc::new(RefCell::new(None::<bool>)));
    use_effect(move || {
        let value = modified_mirror();
        let mut last = modified_forward.borrow_mut();
        if *last == Some(value) {
            return;
        }
        *last = Some(value);
        if let Some(handler) = on_modified {
            handler.call(value);
        }
    });

    // Forward document-change notices with the full text.
    let change_forward = use_hook(|| Rc::new(RefCell::new(0_u64)));
    use_effect(move || {
        let Some(notice) = change_notice() else {
            return;
        };
        let mut last = change_forward.borrow_mut();
        if *last == notice.epoch {
            return;
        }
        *last = notice.epoch;
        if let Some(handler) = on_change {
            handler.call(notice.text);
        }
    });

    // Forward apply requests (Cmd/Ctrl+Enter) with the captured text.
    let apply_forward = use_hook(|| Rc::new(RefCell::new(0_u64)));
    use_effect(move || {
        let Some(request) = apply_request() else {
            return;
        };
        let mut last = apply_forward.borrow_mut();
        if *last == request.epoch {
            return;
        }
        *last = request.epoch;
        if let Some(handler) = on_apply {
            handler.call(request.text);
        }
    });

    // Forward save requests (Cmd/Ctrl+S).
    let save_forward = use_hook(|| Rc::new(RefCell::new(0_u64)));
    use_effect(move || {
        let Some(request) = save_request() else {
            return;
        };
        let mut last = save_forward.borrow_mut();
        if *last == request.epoch {
            return;
        }
        *last = request.epoch;
        if let Some(handler) = on_save {
            handler.call(());
        }
    });

    let shared_for_mount = shared.clone();
    let container_id_for_mount = container_id.clone();
    let onmounted = move |_| {
        let shared = shared_for_mount.clone();
        let container_id = container_id_for_mount.clone();
        spawn(async move {
            initialize_editor(
                shared,
                container_id,
                init_state,
                modified_mirror,
                change_notice,
                apply_request,
                save_request,
            )
            .await;
        });
    };

    let shared_for_drop = shared.clone();
    use_drop(move || {
        if let Some(editor) = shared_for_drop.borrow_mut().editor.take() {
            editor.destroy();
        }
    });

    let story_wait = if matches!(init_state(), CodeEditorInitState::Loading) {
        "1"
    } else {
        "0"
    };

    rsx! {
        div {
            id: "{container_id}",
            class: "ux-code-editor tw:min-h-0 tw:h-full tw:overflow-hidden {class}",
            "data-story-wait": story_wait,
            onmounted,
            if let CodeEditorInitState::Failed(message) = init_state() {
                div { class: "tw:p-2 tw:text-xs tw:text-muted-foreground",
                    "code editor unavailable: {message}"
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum CodeEditorInitState {
    Loading,
    Ready,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
struct ApplyRequest {
    epoch: u64,
    text: String,
}

/// One save request (Cmd/Ctrl+S) from the JS side; epoch-tagged like
/// [`ApplyRequest`] so the forwarding effect fires exactly once per press.
#[derive(Clone, Copy, Debug, PartialEq)]
struct SaveRequest {
    epoch: u64,
}

/// One document-change notification from the JS side, epoch-tagged so the
/// forwarding effect fires the handler exactly once per change.
#[derive(Clone, Debug, PartialEq)]
struct ChangeNotice {
    epoch: u64,
    text: String,
}

/// Latest props, as the reconciliation target for the live editor.
struct DesiredState {
    doc: String,
    read_only: bool,
    diagnostics: Vec<CodeEditorDiagnostic>,
    reveal_line: Option<u32>,
    language: CodeEditorLanguage,
}

struct EditorShared {
    editor: Option<EditorInstance>,
    desired: DesiredState,
}

impl EditorShared {
    /// Push `desired` into the live editor. Called from render (cheap
    /// no-op when in sync) and once right after initialization.
    fn reconcile(&mut self) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        editor.sync_to(&self.desired);
    }
}

/// The live CodeMirror handle plus the callback closures keeping it alive,
/// and the last-synced values so reconciliation only dispatches on change.
struct EditorInstance {
    handle: CmHandle,
    synced_doc: String,
    synced_read_only: bool,
    synced_diagnostics: Vec<CodeEditorDiagnostic>,
    synced_reveal_line: Option<u32>,
    _on_modified: Closure<dyn FnMut(JsValue)>,
    _on_change: Closure<dyn FnMut(JsValue)>,
    _on_apply: Closure<dyn FnMut()>,
    _on_save: Closure<dyn FnMut()>,
}

impl EditorInstance {
    fn sync_to(&mut self, desired: &DesiredState) {
        if desired.doc != self.synced_doc {
            if !self.handle.is_modified() {
                // Unmodified editor: the external doc simply wins.
                self.handle.set_doc(&desired.doc);
            } else if self.handle.get_doc() == desired.doc {
                // The external doc caught up with the user's text (an
                // applied edit acked): the text is no longer "modified".
                self.handle.mark_clean();
            }
            // Otherwise the user's unsaved text wins; the modified chrome
            // stays on and the next external change re-evaluates.
            self.synced_doc = desired.doc.clone();
        }
        if desired.read_only != self.synced_read_only {
            self.handle.set_read_only(desired.read_only);
            self.synced_read_only = desired.read_only;
        }
        if desired.diagnostics != self.synced_diagnostics {
            self.handle.set_diagnostics(&desired.diagnostics);
            self.synced_diagnostics = desired.diagnostics.clone();
        }
        if desired.reveal_line != self.synced_reveal_line {
            if let Some(line) = desired.reveal_line {
                self.handle.reveal_line(line);
            }
            self.synced_reveal_line = desired.reveal_line;
        }
    }

    fn destroy(self) {
        self.handle.destroy();
    }
}

/// Wait for the vendored bundle, then mount CodeMirror into the container.
async fn initialize_editor(
    shared: Rc<RefCell<EditorShared>>,
    container_id: String,
    mut init_state: Signal<CodeEditorInitState>,
    mut modified_mirror: Signal<bool>,
    mut change_notice: Signal<Option<ChangeNotice>>,
    mut apply_request: Signal<Option<ApplyRequest>>,
    mut save_request: Signal<Option<SaveRequest>>,
) {
    if shared.borrow().editor.is_some() {
        return;
    }

    let Some(namespace) = wait_for_bundle().await else {
        init_state.set(CodeEditorInitState::Failed(String::from(
            "vendor bundle /vendor/codemirror/codemirror.js did not load",
        )));
        return;
    };

    let Some(container) = document_element(&container_id) else {
        init_state.set(CodeEditorInitState::Failed(format!(
            "container #{container_id} not found"
        )));
        return;
    };

    let on_modified = Closure::<dyn FnMut(JsValue)>::wrap(Box::new(move |value: JsValue| {
        let value = value.as_bool().unwrap_or(false);
        if modified_mirror.peek().ne(&value) {
            modified_mirror.set(value);
        }
    }));

    let change_epoch = Rc::new(RefCell::new(0_u64));
    let on_change = Closure::<dyn FnMut(JsValue)>::wrap(Box::new(move |value: JsValue| {
        let Some(text) = value.as_string() else {
            return;
        };
        let mut epoch = change_epoch.borrow_mut();
        *epoch += 1;
        change_notice.set(Some(ChangeNotice {
            epoch: *epoch,
            text,
        }));
    }));

    let shared_for_apply = shared.clone();
    let apply_epoch = Rc::new(RefCell::new(0_u64));
    let on_apply = Closure::<dyn FnMut()>::wrap(Box::new(move || {
        let Some(text) = shared_for_apply
            .borrow()
            .editor
            .as_ref()
            .map(|editor| editor.handle.get_doc())
        else {
            return;
        };
        let mut epoch = apply_epoch.borrow_mut();
        *epoch += 1;
        apply_request.set(Some(ApplyRequest {
            epoch: *epoch,
            text,
        }));
    }));

    let save_epoch = Rc::new(RefCell::new(0_u64));
    let on_save = Closure::<dyn FnMut()>::wrap(Box::new(move || {
        let mut epoch = save_epoch.borrow_mut();
        *epoch += 1;
        save_request.set(Some(SaveRequest { epoch: *epoch }));
    }));

    let (doc, language, read_only) = {
        let desired = &shared.borrow().desired;
        (desired.doc.clone(), desired.language, desired.read_only)
    };
    let handle = match CmHandle::create(
        &namespace,
        &container,
        &doc,
        language,
        read_only,
        &on_modified,
        &on_change,
        &on_apply,
        &on_save,
    ) {
        Ok(handle) => handle,
        Err(message) => {
            init_state.set(CodeEditorInitState::Failed(message));
            return;
        }
    };

    {
        let mut shared_mut = shared.borrow_mut();
        shared_mut.editor = Some(EditorInstance {
            handle,
            synced_doc: doc,
            synced_read_only: read_only,
            synced_diagnostics: Vec::new(),
            synced_reveal_line: None,
            _on_modified: on_modified,
            _on_change: on_change,
            _on_apply: on_apply,
            _on_save: on_save,
        });
        // Props may have moved on while the bundle was loading.
        shared_mut.reconcile();
    }
    init_state.set(CodeEditorInitState::Ready);
}

async fn wait_for_bundle() -> Option<js_sys::Object> {
    for _ in 0..BUNDLE_POLL_LIMIT {
        if let Some(namespace) = lp_codemirror_namespace() {
            return Some(namespace);
        }
        gloo_timers::future::TimeoutFuture::new(BUNDLE_POLL_INTERVAL_MS).await;
    }
    lp_codemirror_namespace()
}

fn lp_codemirror_namespace() -> Option<js_sys::Object> {
    js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("LpCodeMirror"))
        .ok()
        .filter(|value| !value.is_undefined() && !value.is_null())
        .and_then(|value| value.dyn_into::<js_sys::Object>().ok())
}

fn document_element(id: &str) -> Option<web_sys::Element> {
    web_sys::window()?.document()?.get_element_by_id(id)
}

/// Thin typed wrapper over the JS handle returned by
/// `LpCodeMirror.createEditor` (see `vendor-src/codemirror/entry.mjs` for
/// the contract). All calls are best-effort: a JS exception is logged and
/// swallowed rather than crashing the app over editor chrome.
struct CmHandle(js_sys::Object);

impl CmHandle {
    #[allow(
        clippy::too_many_arguments,
        reason = "one-shot constructor mirroring the JS options object"
    )]
    fn create(
        namespace: &js_sys::Object,
        parent: &web_sys::Element,
        doc: &str,
        language: CodeEditorLanguage,
        read_only: bool,
        on_modified: &Closure<dyn FnMut(JsValue)>,
        on_change: &Closure<dyn FnMut(JsValue)>,
        on_apply: &Closure<dyn FnMut()>,
        on_save: &Closure<dyn FnMut()>,
    ) -> Result<Self, String> {
        let create = js_sys::Reflect::get(namespace, &JsValue::from_str("createEditor"))
            .ok()
            .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
            .ok_or_else(|| String::from("LpCodeMirror.createEditor is not a function"))?;

        let opts = js_sys::Object::new();
        let set = |key: &str, value: &JsValue| {
            // Reflect::set on a fresh plain object cannot fail.
            let _ = js_sys::Reflect::set(&opts, &JsValue::from_str(key), value);
        };
        set("doc", &JsValue::from_str(doc));
        set("language", &JsValue::from_str(language.as_str()));
        set("readOnly", &JsValue::from_bool(read_only));
        set("onModified", on_modified.as_ref());
        set("onChange", on_change.as_ref());
        set("onApplyRequested", on_apply.as_ref());
        set("onSaveRequested", on_save.as_ref());

        let handle = create
            .call2(namespace, parent, &opts)
            .map_err(|err| format!("createEditor failed: {err:?}"))?;
        handle
            .dyn_into::<js_sys::Object>()
            .map(Self)
            .map_err(|_| String::from("createEditor returned a non-object"))
    }

    fn get_doc(&self) -> String {
        self.call0("getDoc")
            .and_then(|value| value.as_string())
            .unwrap_or_default()
    }

    fn set_doc(&self, text: &str) {
        self.call1("setDoc", &JsValue::from_str(text));
    }

    fn mark_clean(&self) {
        self.call0("markClean");
    }

    fn is_modified(&self) -> bool {
        self.call0("isModified")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    }

    fn set_read_only(&self, value: bool) {
        self.call1("setReadOnly", &JsValue::from_bool(value));
    }

    fn set_diagnostics(&self, diagnostics: &[CodeEditorDiagnostic]) {
        let list = js_sys::Array::new();
        for diagnostic in diagnostics {
            let entry = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &entry,
                &JsValue::from_str("line"),
                &JsValue::from_f64(f64::from(diagnostic.line)),
            );
            let _ = js_sys::Reflect::set(
                &entry,
                &JsValue::from_str("col"),
                &JsValue::from_f64(f64::from(diagnostic.col)),
            );
            let _ = js_sys::Reflect::set(
                &entry,
                &JsValue::from_str("message"),
                &JsValue::from_str(&diagnostic.message),
            );
            list.push(&entry);
        }
        self.call1("setDiagnostics", &list);
    }

    fn reveal_line(&self, line: u32) {
        self.call1("revealLine", &JsValue::from_f64(f64::from(line)));
    }

    fn destroy(&self) {
        self.call0("destroy");
    }

    fn call0(&self, name: &str) -> Option<JsValue> {
        let method = js_sys::Reflect::get(&self.0, &JsValue::from_str(name))
            .ok()
            .and_then(|value| value.dyn_into::<js_sys::Function>().ok())?;
        match method.call0(&self.0) {
            Ok(value) => Some(value),
            Err(err) => {
                log::warn!("[code-editor] {name} failed: {err:?}");
                None
            }
        }
    }

    fn call1(&self, name: &str, arg: &JsValue) -> Option<JsValue> {
        let method = js_sys::Reflect::get(&self.0, &JsValue::from_str(name))
            .ok()
            .and_then(|value| value.dyn_into::<js_sys::Function>().ok())?;
        match method.call1(&self.0, arg) {
            Ok(value) => Some(value),
            Err(err) => {
                log::warn!("[code-editor] {name} failed: {err:?}");
                None
            }
        }
    }
}
