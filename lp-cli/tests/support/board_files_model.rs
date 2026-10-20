//! The soak's oracle: what a board's files may be after a power cut, given
//! what they were and what the host asked for since (plan
//! `lp2025/2026-10-08-1017-tree-store-device-round`, M7).
//!
//! **The rule (the round's invariant).** After any cut and remount, every
//! file reads as its old or its new content, and a transaction is old or new
//! as a whole:
//!
//! - A change the board **acknowledged** is durable: a save answered `Ok`, a
//!   push whose `CommitBatch` was answered. After it, only it (or a later
//!   change still in flight) may be read.
//! - A change **in flight** at the cut may be read as before or after.
//! - A change the board makes **on its own** after a request (a panel
//!   value's auto-save, `/lightplayer.json`'s `startup_project` once a load
//!   has run its first frames) is never acknowledged, so it is always "old
//!   or new": a panel file holds a value that was written, `/lightplayer.json`
//!   names a project that was loaded.
//! - A push is one batch: the slot's files are exactly the old set or
//!   exactly the new one, never a mix.
//!
//! The base is the last state read back and checked, byte for byte; every
//! cycle starts from it.

#![allow(dead_code, reason = "each test file uses part of this")]

use std::collections::{BTreeMap, BTreeSet};

/// A file set: absolute path → bytes.
pub type Files = BTreeMap<String, Vec<u8>>;

/// One change to the board's files.
#[derive(Clone, Debug)]
pub enum Change {
    /// A whole file written.
    Put(String, Vec<u8>),
    /// Every file under this directory (a trailing `/`) removed.
    RemoveTree(String),
    /// A panel value persisted into this `panel.json` by the auto-save.
    Panel { path: String, value: f32 },
    /// `/lightplayer.json` naming `projects/<slot>`.
    Startup { slot: String },
}

/// What the host asked for, and what came back.
#[derive(Clone, Debug)]
pub struct Event {
    pub label: String,
    pub changes: Vec<Change>,
    /// The board answered and the change is synchronous: it must be there.
    pub durable: bool,
    /// A batch over this slot directory (`/projects/<slot>/`): its files
    /// are old or new as a whole.
    pub atomic_slot: Option<String>,
}

/// What a path may read as.
#[derive(Clone, Debug)]
enum Allowed {
    Exact(Option<Vec<u8>>),
    PanelValue(f32),
    StartupSlot(String),
}

/// How one event landed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Landed {
    Old,
    New,
    /// It changed nothing visible (the same bytes, or nothing to change).
    Same,
}

/// The verdict on a read-back.
#[derive(Debug, Default)]
pub struct Verdict {
    pub failures: Vec<String>,
    /// Per event (by index): old, new or same.
    pub landed: Vec<Option<Landed>>,
}

/// Judge `observed` against `base` and the cycle's `events`.
pub fn check(base: &Files, events: &[Event], observed: &Files) -> Verdict {
    let mut verdict = Verdict::default();
    let mut paths: BTreeSet<String> = base.keys().cloned().collect();
    paths.extend(observed.keys().cloned());
    for e in events {
        for c in &e.changes {
            match c {
                Change::Put(p, _) => {
                    paths.insert(p.clone());
                }
                Change::Panel { path, .. } => {
                    paths.insert(path.clone());
                }
                Change::Startup { .. } => {
                    paths.insert("/lightplayer.json".to_string());
                }
                Change::RemoveTree(_) => {}
            }
        }
    }
    for path in &paths {
        let mut allowed = vec![Allowed::Exact(base.get(path).cloned())];
        for e in events {
            let Some(v) = effect(e, path) else { continue };
            if e.durable {
                allowed = vec![v];
            } else {
                allowed.push(v);
            }
        }
        let got = observed.get(path);
        if !allowed.iter().any(|a| matches(a, got)) {
            verdict.failures.push(format!(
                "{path}: read {} — allowed {}",
                describe(got),
                allowed
                    .iter()
                    .map(describe_allowed)
                    .collect::<Vec<_>>()
                    .join(" | ")
            ));
        }
    }
    // A batch is old or new as a whole.
    for e in events.iter().filter(|e| !e.durable) {
        let Some(slot) = &e.atomic_slot else { continue };
        let prefix = format!("/projects/{slot}/");
        let old = slot_files(base, &prefix);
        let new: Files = e
            .changes
            .iter()
            .filter_map(|c| match c {
                Change::Put(p, b) if p.starts_with(&prefix) && !p.contains("/.lp/") => {
                    Some((p.clone(), b.clone()))
                }
                _ => None,
            })
            .collect();
        let got = slot_files(observed, &prefix);
        if got != old && got != new {
            verdict.failures.push(format!(
                "batch `{}` over {prefix} is neither old nor new: {}",
                e.label,
                mix(&old, &new, &got)
            ));
        }
    }
    verdict.landed = events.iter().map(|e| landed(base, e, observed)).collect();
    verdict
}

/// The slot's project files (its `.lp/` state aside).
pub fn slot_files(files: &Files, prefix: &str) -> Files {
    files
        .iter()
        .filter(|(p, _)| p.starts_with(prefix) && !p.contains("/.lp/"))
        .map(|(p, b)| (p.clone(), b.clone()))
        .collect()
}

/// What `e` does to `path`, if anything (the last of its changes wins).
fn effect(e: &Event, path: &str) -> Option<Allowed> {
    let mut out = None;
    for c in &e.changes {
        match c {
            Change::Put(p, b) if p == path => out = Some(Allowed::Exact(Some(b.clone()))),
            Change::RemoveTree(prefix) if path.starts_with(prefix.as_str()) => {
                out = Some(Allowed::Exact(None))
            }
            Change::Panel { path: p, value } if p == path => {
                out = Some(Allowed::PanelValue(*value))
            }
            Change::Startup { slot } if path == "/lightplayer.json" => {
                out = Some(Allowed::StartupSlot(slot.clone()))
            }
            _ => {}
        }
    }
    out
}

fn matches(a: &Allowed, got: Option<&Vec<u8>>) -> bool {
    match a {
        Allowed::Exact(want) => want.as_ref() == got,
        Allowed::PanelValue(v) => {
            got.is_some_and(|b| String::from_utf8_lossy(b).contains(&format!("{{\"f32\":{v}}}")))
        }
        Allowed::StartupSlot(slot) => got.is_some_and(|b| startup_slot(b).as_deref() == Some(slot)),
    }
}

/// The slot `/lightplayer.json` names (`projects/<slot>` → `<slot>`).
pub fn startup_slot(bytes: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let named = v.get("startup_project")?.as_str()?;
    Some(
        named
            .trim_start_matches('/')
            .trim_start_matches("projects/")
            .to_string(),
    )
}

fn landed(base: &Files, e: &Event, observed: &Files) -> Option<Landed> {
    // Judged on the paths the event names, by whether the read-back shows
    // its effect.
    let mut saw_new = false;
    let mut saw_old = false;
    let mut touched = false;
    let mut paths: BTreeSet<String> = BTreeSet::new();
    for c in &e.changes {
        match c {
            Change::Put(p, _) => {
                paths.insert(p.clone());
            }
            Change::Panel { path, .. } => {
                paths.insert(path.clone());
            }
            Change::Startup { .. } => {
                paths.insert("/lightplayer.json".to_string());
            }
            Change::RemoveTree(prefix) => {
                paths.extend(
                    base.keys()
                        .filter(|p| p.starts_with(prefix.as_str()))
                        .cloned(),
                );
            }
        }
    }
    for p in &paths {
        let Some(v) = effect(e, p) else { continue };
        let old = Allowed::Exact(base.get(p).cloned());
        let got = observed.get(p);
        let is_new = matches(&v, got);
        let is_old = matches(&old, got);
        if is_new && is_old {
            continue;
        }
        touched = true;
        saw_new |= is_new;
        saw_old |= is_old;
    }
    Some(match (touched, saw_new, saw_old) {
        (false, _, _) => Landed::Same,
        (true, true, false) => Landed::New,
        (true, false, true) => Landed::Old,
        // A mix: per-file it may still pass (the panel and the startup are
        // separate files); the failures say whether it was allowed.
        _ => return None,
    })
}

fn describe(b: Option<&Vec<u8>>) -> String {
    match b {
        None => "absent".to_string(),
        Some(b) => format!("{} B sha {}", b.len(), short_hash(b)),
    }
}

fn describe_allowed(a: &Allowed) -> String {
    match a {
        Allowed::Exact(b) => describe(b.as_ref()),
        Allowed::PanelValue(v) => format!("a panel file holding {v}"),
        Allowed::StartupSlot(s) => format!("a config naming {s}"),
    }
}

fn mix(old: &Files, new: &Files, got: &Files) -> String {
    let mut parts = Vec::new();
    let keys: BTreeSet<&String> = old.keys().chain(new.keys()).chain(got.keys()).collect();
    for k in keys {
        let g = got.get(k);
        let tag = match (g == old.get(k), g == new.get(k)) {
            (true, true) => continue,
            (true, false) => "old",
            (false, true) => "new",
            (false, false) => "neither",
        };
        parts.push(format!("{k}={tag}"));
    }
    parts.join(", ")
}

/// A short content tag for messages (FNV-1a, not a security hash).
pub fn short_hash(b: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for x in b {
        h ^= u64::from(*x);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{:08x}", h >> 32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(pairs: &[(&str, &str)]) -> Files {
        pairs
            .iter()
            .map(|(p, b)| (p.to_string(), b.as_bytes().to_vec()))
            .collect()
    }

    #[test]
    fn an_acked_save_must_be_there() {
        let base = files(&[("/projects/demo/shader.glsl", "a")]);
        let e = Event {
            label: "save".into(),
            changes: vec![Change::Put(
                "/projects/demo/shader.glsl".into(),
                b"b".to_vec(),
            )],
            durable: true,
            atomic_slot: None,
        };
        assert!(!check(&base, &[e.clone()], &base).failures.is_empty());
        let after = files(&[("/projects/demo/shader.glsl", "b")]);
        assert!(check(&base, &[e], &after).failures.is_empty());
    }

    #[test]
    fn an_unacked_batch_is_old_or_new_as_a_whole() {
        let base = files(&[("/projects/demo/a", "1"), ("/projects/demo/b", "1")]);
        let e = Event {
            label: "push".into(),
            changes: vec![
                Change::RemoveTree("/projects/demo/".into()),
                Change::Put("/projects/demo/a".into(), b"2".to_vec()),
                Change::Put("/projects/demo/b".into(), b"2".to_vec()),
            ],
            durable: false,
            atomic_slot: Some("demo".into()),
        };
        let new = files(&[("/projects/demo/a", "2"), ("/projects/demo/b", "2")]);
        let mixed = files(&[("/projects/demo/a", "2"), ("/projects/demo/b", "1")]);
        assert!(check(&base, &[e.clone()], &base).failures.is_empty());
        assert!(check(&base, &[e.clone()], &new).failures.is_empty());
        assert_eq!(check(&base, &[e], &mixed).failures.len(), 1);
    }

    #[test]
    fn a_panel_file_holds_a_written_value_or_its_old_bytes() {
        let p = "/projects/demo/.lp/panel.json";
        let base = files(&[(p, "{\"time\":{\"f32\":1.5}}")]);
        let e = Event {
            label: "panel".into(),
            changes: vec![Change::Panel {
                path: p.into(),
                value: 7.5,
            }],
            durable: false,
            atomic_slot: None,
        };
        let new = files(&[(p, "{\"time\":{\"f32\":7.5}}")]);
        let other = files(&[(p, "{\"time\":{\"f32\":9.5}}")]);
        assert!(check(&base, &[e.clone()], &new).failures.is_empty());
        assert!(check(&base, &[e.clone()], &base).failures.is_empty());
        assert!(!check(&base, &[e], &other).failures.is_empty());
    }
}
