//! The oracle: a path → bytes model, and the checks a store must pass after a
//! power cut.
//!
//! Required for eligibility: every path is its old or its new value for the
//! interrupted step (absent counts), and no path outside old ∪ new appears.
//! Scored, not required: the whole state equals old or new (step atomicity).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{CandidateStore, Op, Step, StoreError};

/// What the store should hold.
pub type Model = BTreeMap<String, Arc<Vec<u8>>>;

/// Apply a step to the model.
pub fn apply_step(model: &mut Model, step: &Step) {
    for op in &step.ops {
        match op {
            Op::Put { path, bytes } => {
                model.insert(path.clone(), bytes.clone());
            }
            Op::DeletePrefix(prefix) => model.retain(|p, _| !p.starts_with(prefix.as_str())),
        }
    }
}

/// Run a step's operations, then `commit`.
pub fn run_step(store: &mut dyn CandidateStore, step: &Step) -> Result<(), StoreError> {
    for op in &step.ops {
        match op {
            Op::Put { path, bytes } => store.put(path, bytes)?,
            Op::DeletePrefix(prefix) => store.delete_prefix(prefix)?,
        }
    }
    store.commit()
}

/// Why a case failed. The `kind` is the scoreboard's bucket.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    pub kind: String,
    pub detail: String,
}

impl Failure {
    pub fn new(kind: &str, detail: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            detail: detail.into(),
        }
    }
}

/// Read the store's whole visible state: every listed path and every path in
/// `expect_paths`, with list/get consistency checked.
pub fn read_state(
    store: &mut dyn CandidateStore,
    expect_paths: &BTreeSet<String>,
) -> Result<Model, Failure> {
    let listed = store
        .list("/")
        .map_err(|e| Failure::new("read_error", format!("list: {e}")))?;
    let listed: BTreeSet<String> = listed.into_iter().collect();
    let mut state = Model::new();
    for p in listed.iter().chain(expect_paths.iter()) {
        if state.contains_key(p) {
            continue;
        }
        match store.get(p) {
            Ok(Some(b)) => {
                if !listed.contains(p) {
                    return Err(Failure::new(
                        "list_mismatch",
                        format!("{p} readable but not listed"),
                    ));
                }
                state.insert(p.clone(), Arc::new(b));
            }
            Ok(None) => {
                if listed.contains(p) {
                    return Err(Failure::new(
                        "list_mismatch",
                        format!("{p} listed but not readable"),
                    ));
                }
            }
            Err(e) => return Err(Failure::new("read_error", format!("get {p}: {e}"))),
        }
    }
    Ok(state)
}

/// The per-document check: `Ok(atomic)` when every path is old-or-new.
pub fn judge_old_or_new(state: &Model, old: &Model, new: &Model) -> Result<bool, Failure> {
    let all: BTreeSet<&String> = state.keys().chain(old.keys()).chain(new.keys()).collect();
    for p in all {
        let (s, o, n) = (state.get(p), old.get(p), new.get(p));
        if o.is_none() && n.is_none() {
            return Err(Failure::new("unknown_path", format!("{p} appeared")));
        }
        if s != o && s != n {
            let what = match s {
                None => "missing".to_string(),
                Some(b) => format!("{} B, neither old nor new", b.len()),
            };
            return Err(Failure::new(
                "doc_not_old_or_new",
                format!(
                    "{p}: {what} (old {:?} B, new {:?} B)",
                    o.map(|b| b.len()),
                    n.map(|b| b.len())
                ),
            ));
        }
    }
    Ok(state == old || state == new)
}

/// Every path in old ∪ new.
pub fn paths_of(old: &Model, new: &Model) -> BTreeSet<String> {
    old.keys().chain(new.keys()).cloned().collect()
}

/// First difference between two states, for a failure's detail.
pub fn first_diff(got: &Model, want: &Model) -> String {
    let all: BTreeSet<&String> = got.keys().chain(want.keys()).collect();
    for p in all {
        let (g, w) = (got.get(p), want.get(p));
        if g != w {
            return format!(
                "{p}: got {:?} B, want {:?} B",
                g.map(|b| b.len()),
                w.map(|b| b.len())
            );
        }
    }
    "no difference".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pairs: &[(&str, &[u8])]) -> Model {
        pairs
            .iter()
            .map(|(p, b)| (p.to_string(), Arc::new(b.to_vec())))
            .collect()
    }

    #[test]
    fn old_or_new_per_document() {
        let old = m(&[("/a", b"1"), ("/b", b"1")]);
        let new = m(&[("/a", b"2"), ("/b", b"2")]);
        assert_eq!(judge_old_or_new(&old, &old, &new), Ok(true));
        assert_eq!(judge_old_or_new(&new, &old, &new), Ok(true));
        let mixed = m(&[("/a", b"2"), ("/b", b"1")]);
        assert_eq!(judge_old_or_new(&mixed, &old, &new), Ok(false));
        let bad = m(&[("/a", b"3"), ("/b", b"1")]);
        assert_eq!(
            judge_old_or_new(&bad, &old, &new).unwrap_err().kind,
            "doc_not_old_or_new"
        );
        let extra = m(&[("/a", b"1"), ("/b", b"1"), ("/c", b"x")]);
        assert_eq!(
            judge_old_or_new(&extra, &old, &new).unwrap_err().kind,
            "unknown_path"
        );
    }

    #[test]
    fn absent_counts_as_a_value() {
        let old = m(&[]);
        let new = m(&[("/a", b"1")]);
        assert_eq!(judge_old_or_new(&m(&[]), &old, &new), Ok(true));
        let lost = m(&[]);
        assert!(judge_old_or_new(&lost, &new, &new).is_err());
    }

    #[test]
    fn delete_prefix_applies_to_the_model() {
        let mut model = m(&[
            ("/projects/a/x", b"1"),
            ("/projects/ab", b"1"),
            ("/h", b"1"),
        ]);
        let mut s = Step::new("d");
        s.delete_prefix("/projects/a/");
        apply_step(&mut model, &s);
        assert_eq!(model.len(), 2);
    }
}
