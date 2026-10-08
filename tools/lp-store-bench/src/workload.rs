//! Workloads: steps built from corpora at runtime, deterministic by seed, so a
//! reproducer names a workload by its [`WorkloadSpec`] alone.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use lp_nor_sim::SimRng;
use serde::{Deserialize, Serialize};

use crate::{Corpus, Step, board_files};

/// The workload families of the testbed (P2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadKind {
    /// Board files, then the corpus pushed into an empty store.
    Push,
    /// …then pushed again whole with 3 shaders changed ±50 B.
    Repush,
    /// …then 20 saves of 1–3 documents each.
    Save,
    /// …then 100 rewrites of the project's `.lp/panel.json`.
    Panel,
    /// Project A (first corpus) replaced by B (second) and back.
    Switch,
}

impl WorkloadKind {
    pub const ALL: [WorkloadKind; 5] = [
        WorkloadKind::Push,
        WorkloadKind::Repush,
        WorkloadKind::Save,
        WorkloadKind::Panel,
        WorkloadKind::Switch,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            WorkloadKind::Push => "push",
            WorkloadKind::Repush => "repush",
            WorkloadKind::Save => "save",
            WorkloadKind::Panel => "panel",
            WorkloadKind::Switch => "switch",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == s)
    }
}

/// Everything needed to rebuild a workload: kind, corpus (`c40`; for
/// `switch`, `a,b`; `syn:<modules>:<shader_len>` for a synthetic one), seed.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkloadSpec {
    pub kind: WorkloadKind,
    pub corpus: String,
    pub seed: u64,
}

impl WorkloadSpec {
    pub fn new(kind: WorkloadKind, corpus: &str, seed: u64) -> Self {
        Self {
            kind,
            corpus: corpus.into(),
            seed,
        }
    }

    /// `push:c40`, `switch:c13,c40reuse`, … (seed 1 unless `@<seed>` follows).
    pub fn parse(s: &str) -> Option<Self> {
        let (body, seed) = match s.split_once('@') {
            Some((b, seed)) => (b, seed.parse().ok()?),
            None => (s, 1),
        };
        let (kind, corpus) = body.split_once(':')?;
        Some(Self::new(WorkloadKind::from_name(kind)?, corpus, seed))
    }

    pub fn label(&self) -> String {
        format!("{}:{}", self.kind.name(), self.corpus)
    }
}

/// A built workload: steps, and the first step the sweeps focus on (the steps
/// before it are setup: board files, the first push).
#[derive(Clone, Debug)]
pub struct Workload {
    pub spec: WorkloadSpec,
    pub steps: Vec<Step>,
    pub focus: usize,
}

/// Where corpora come from: a directory of corpus directories, plus the
/// synthetic ones, loaded once and shared.
#[derive(Debug)]
pub struct CorpusSet {
    root: Option<PathBuf>,
    cache: Mutex<HashMap<String, Arc<Corpus>>>,
}

impl CorpusSet {
    pub fn new(root: Option<PathBuf>) -> Self {
        Self {
            root,
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn get(&self, name: &str) -> Result<Arc<Corpus>, String> {
        if let Some(c) = self.cache.lock().unwrap().get(name) {
            return Ok(c.clone());
        }
        let corpus = if let Some(rest) = name.strip_prefix("syn:") {
            let mut it = rest.split(':');
            let modules = it.next().and_then(|v| v.parse().ok()).unwrap_or(4);
            let len = it.next().and_then(|v| v.parse().ok()).unwrap_or(600);
            Corpus::synthetic(name, modules, len, 0xC0FFEE)
        } else {
            let root = self
                .root
                .as_ref()
                .ok_or_else(|| format!("corpus {name}: no --corpus dir"))?;
            Corpus::load(&root.join(name)).map_err(|e| format!("corpus {name}: {e}"))?
        };
        let corpus = Arc::new(corpus);
        self.cache
            .lock()
            .unwrap()
            .insert(name.into(), corpus.clone());
        Ok(corpus)
    }

    /// Build the steps of `spec`.
    pub fn build(&self, spec: &WorkloadSpec) -> Result<Workload, String> {
        let mut rng = SimRng::new(spec.seed);
        let mut steps = vec![board_step()];
        let focus;
        match spec.kind {
            WorkloadKind::Switch => {
                let (a, b) = spec
                    .corpus
                    .split_once(',')
                    .ok_or("switch needs two corpora: a,b")?;
                let (a, b) = (self.get(a)?, self.get(b)?);
                steps.push(push_step("push-a", "a", &a, &BTreeMap::new()));
                let mut to_b = push_step("switch-to-b", "b", &b, &BTreeMap::new());
                to_b.ops
                    .insert(0, crate::Op::DeletePrefix("/projects/a/".into()));
                steps.push(to_b);
                let mut to_a = push_step("switch-to-a", "a", &a, &BTreeMap::new());
                to_a.ops
                    .insert(0, crate::Op::DeletePrefix("/projects/b/".into()));
                steps.push(to_a);
                focus = 2;
            }
            kind => {
                let c = self.get(&spec.corpus)?;
                let mut cur: BTreeMap<String, Arc<Vec<u8>>> = BTreeMap::new();
                let mut push = push_step("push", "a", &c, &BTreeMap::new());
                if kind == WorkloadKind::Panel {
                    push.put(PANEL_PATH, Arc::new(panel_json(&mut rng)));
                }
                for op in &push.ops {
                    if let crate::Op::Put { path, bytes } = op {
                        cur.insert(path.clone(), bytes.clone());
                    }
                }
                steps.push(push);
                focus = if kind == WorkloadKind::Push { 0 } else { 2 };
                match kind {
                    WorkloadKind::Push => {}
                    WorkloadKind::Repush => {
                        let shaders: Vec<&String> =
                            cur.keys().filter(|p| p.ends_with(".glsl")).collect();
                        let mut edits = BTreeMap::new();
                        for _ in 0..3.min(shaders.len()) {
                            let p = shaders[rng.below(shaders.len() as u64) as usize];
                            let rel = p.trim_start_matches("/projects/a/").to_string();
                            edits.insert(rel, Arc::new(edit_doc(p, &cur[p], &mut rng)));
                        }
                        let mut re = push_step("repush", "a", &c, &edits);
                        re.ops
                            .insert(0, crate::Op::DeletePrefix("/projects/a/".into()));
                        steps.push(re);
                    }
                    WorkloadKind::Save => {
                        let docs: Vec<String> = cur.keys().cloned().collect();
                        for i in 0..20 {
                            let mut st = Step::new(format!("save-{i}"));
                            for _ in 0..1 + rng.below(3) {
                                let p = &docs[rng.below(docs.len() as u64) as usize];
                                let b = Arc::new(edit_doc(p, &cur[p], &mut rng));
                                cur.insert(p.clone(), b.clone());
                                st.put(p.clone(), b);
                            }
                            steps.push(st);
                        }
                    }
                    WorkloadKind::Panel => {
                        for i in 0..100 {
                            let mut st = Step::new(format!("panel-{i}"));
                            st.put(PANEL_PATH, Arc::new(panel_json(&mut rng)));
                            steps.push(st);
                        }
                    }
                    WorkloadKind::Switch => unreachable!(),
                }
            }
        }
        Ok(Workload {
            spec: spec.clone(),
            steps,
            focus,
        })
    }
}

/// The hot document: Studio's panel state for project `a`.
pub const PANEL_PATH: &str = "/projects/a/.lp/panel.json";

/// The board files, as one step.
pub fn board_step() -> Step {
    let mut s = Step::new("board");
    for d in board_files() {
        s.put(d.rel, d.bytes);
    }
    s
}

/// Put every doc of `corpus` under `/projects/<slot>/`, substituting `edits`
/// (keyed by relative path).
pub fn push_step(
    label: &str,
    slot: &str,
    corpus: &Corpus,
    edits: &BTreeMap<String, Arc<Vec<u8>>>,
) -> Step {
    let mut s = Step::new(label);
    for d in &corpus.docs {
        let bytes = edits
            .get(&d.rel)
            .cloned()
            .unwrap_or_else(|| d.bytes.clone());
        s.put(format!("/projects/{slot}/{}", d.rel), bytes);
    }
    s
}

/// A ~600 B pretty-printed panel document with seeded knob values.
pub fn panel_json(rng: &mut SimRng) -> Vec<u8> {
    let mut s = String::from("{\n  \"knobs\": {\n");
    const NAMES: [&str; 14] = [
        "speed",
        "palette",
        "phase",
        "brightness",
        "hue",
        "saturation",
        "scale",
        "warp",
        "density",
        "glow",
        "contrast",
        "offset",
        "spin",
        "fade",
    ];
    for (i, n) in NAMES.iter().enumerate() {
        let v = rng.below(1000);
        s.push_str(&format!(
            "    \"{n}\": {}.{:03}{}\n",
            v / 1000,
            v % 1000,
            if i + 1 < NAMES.len() { "," } else { "" }
        ));
    }
    s.push_str("  },\n  \"page\": \"main\",\n  \"version\": 2\n}\n");
    s.into_bytes()
}

/// A seeded edit of about ±50 B: a shader gains or loses a trailing comment;
/// a JSON document has up to three digits rewritten (its shape stays the same,
/// so a re-printing store still sees canonical JSON).
pub fn edit_doc(path: &str, old: &[u8], rng: &mut SimRng) -> Vec<u8> {
    let mut b = old.to_vec();
    if path.ends_with(".json") {
        let digits: Vec<usize> = (0..b.len()).filter(|&i| b[i].is_ascii_digit()).collect();
        if !digits.is_empty() {
            for _ in 0..3 {
                let i = digits[rng.below(digits.len() as u64) as usize];
                b[i] = b'0' + rng.below(10) as u8;
            }
            if b != old {
                return b;
            }
        }
        // No digit to change (or it landed on the same value): grow a string.
        if let Some(q) = b.iter().rposition(|&c| c == b'"') {
            b.insert(q, b'x');
        } else {
            b.push(b' ');
        }
        return b;
    }
    let delta = 1 + rng.below(50) as usize;
    if rng.chance(1, 2) || b.len() < 200 {
        let at = b.len().saturating_sub(1);
        let mut add = b"// ".to_vec();
        while add.len() < delta {
            add.push(b'a' + rng.below(26) as u8);
        }
        add.push(b'\n');
        for (i, c) in add.into_iter().enumerate() {
            b.insert(at + i, c);
        }
    } else {
        let end = b.len() - 1;
        b.drain(end - delta..end);
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workloads_build_deterministically_from_a_synthetic_corpus() {
        let set = CorpusSet::new(None);
        for kind in WorkloadKind::ALL {
            let corpus = if kind == WorkloadKind::Switch {
                "syn:3:400,syn:5:300"
            } else {
                "syn:4:500"
            };
            let spec = WorkloadSpec::new(kind, corpus, 7);
            let a = set.build(&spec).unwrap();
            let b = set.build(&spec).unwrap();
            assert_eq!(a.steps, b.steps, "{kind:?}");
            assert!(a.focus < a.steps.len());
        }
        let save = set
            .build(&WorkloadSpec::new(WorkloadKind::Save, "syn:4:500", 7))
            .unwrap();
        assert_eq!(save.steps.len(), 2 + 20);
    }

    #[test]
    fn edits_change_bytes_by_at_most_about_fifty() {
        let mut rng = SimRng::new(3);
        let glsl = vec![b'a'; 600];
        for _ in 0..50 {
            let e = edit_doc("x.glsl", &glsl, &mut rng);
            assert_ne!(e, glsl);
            assert!((e.len() as i64 - 600).abs() <= 52);
        }
        let json = b"{\n  \"a\": 12\n}\n".to_vec();
        let e = edit_doc("x.json", &json, &mut rng);
        assert_ne!(e, json);
    }

    #[test]
    fn panel_is_about_six_hundred_bytes() {
        let n = panel_json(&mut SimRng::new(1)).len();
        assert!((300..800).contains(&n), "{n}");
    }

    #[test]
    fn spec_parses() {
        let s = WorkloadSpec::parse("switch:c13,c40reuse@3").unwrap();
        assert_eq!(s.kind, WorkloadKind::Switch);
        assert_eq!(s.corpus, "c13,c40reuse");
        assert_eq!(s.seed, 3);
    }
}
