//! The piece: the real install, as it is really wired (plan D4).
//!
//! Patching by hand is about the gap between the model and the install:
//! "which panel is on this port, which way does its data run, where does it
//! enter?" The lab's board is simulated, so in the lab that gap is made on
//! purpose: the piece is wired by a **truth**, a patch the person can't
//! see — in the lab, the project's patch as it was opened. What the piece
//! shows is the board's pixels through the truth: each lamp takes the
//! colour of the pixel it is really on. Studio needs none of this; there
//! the piece is the real thing.
//!
//! The truth also says when a hand patch is done: when the project's patch
//! puts every lamp where the truth does ([`Piece::matches`]).

use std::collections::{HashMap, HashSet};

use crate::domain::board::BoardFrame;
use crate::domain::node::NodeId;
use crate::domain::part_ref::TreeRef;
use crate::domain::patch::{EffectivePatch, Landing};
use crate::domain::project::Project;
use crate::domain::rgb::Rgb;

/// An install, wired as a truth says.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    truth: EffectivePatch,
    /// Each object's spans in `truth`.
    by_object: HashMap<TreeRef, Vec<usize>>,
    /// Where the truth came from, in words: `dome.patch.json when it
    /// opened`.
    pub wired_as: String,
}

/// One object the project's patch puts somewhere the piece doesn't.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    pub object: TreeRef,
    pub kind: DifferenceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DifferenceKind {
    /// The piece has it on the wire; the patch has it nowhere.
    NotPatched,
    /// On other pixels than the piece's (or the piece has it nowhere).
    OtherPlace,
    /// On the right pixels, running the other way.
    Reversed,
    /// On the right pixels, the right way, entering at another lamp.
    Turned,
}

impl Piece {
    /// A piece wired as `truth` says.
    pub fn wired(truth: EffectivePatch, wired_as: impl Into<String>) -> Self {
        let mut by_object: HashMap<TreeRef, Vec<usize>> = HashMap::new();
        for (i, s) in truth.pixel_runs.iter().enumerate() {
            by_object.entry(s.segment.clone()).or_default().push(i);
        }
        Piece {
            truth,
            by_object,
            wired_as: wired_as.into(),
        }
    }

    /// A piece wired as the project's patch is now.
    pub fn of(project: &Project, wired_as: impl Into<String>) -> Self {
        Self::wired(project.effective_patch().clone(), wired_as)
    }

    /// A piece wired the way an install really is when nobody wrote it
    /// down: every object the project has on `output`'s ports, shuffled
    /// across those same places, each running a random way round and
    /// entering at a random lamp. Seeded, so a test and a person at the
    /// gate get the same install.
    pub fn scrambled(
        project: &Project,
        output: &NodeId,
        seed: u64,
        wired_as: impl Into<String>,
    ) -> Self {
        let patch = project.effective_patch();
        // Each object on the output, and where its first span starts.
        let mut objects: Vec<TreeRef> = Vec::new();
        let mut places: Vec<(String, u32)> = Vec::new();
        for s in patch.pixel_runs.iter().filter(|s| &s.output == output) {
            if objects.contains(&s.segment) {
                continue;
            }
            objects.push(s.segment.clone());
            places.push((s.port.clone(), s.pixels().start));
        }
        let mut rng = Lcg(seed);
        for i in (1..places.len()).rev() {
            let j = (rng.next() % (i as u64 + 1)) as usize;
            places.swap(i, j);
        }
        let mut wired = project.clone();
        for o in &objects {
            wired.unpatch_segment(o);
        }
        for (o, (port, pixel)) in objects.iter().zip(places) {
            wired.patch_segment(o, output, &port, pixel);
            if rng.next() % 2 == 1 {
                wired.reverse_placement(o);
            }
            let n = wired.segment(o).map_or(1, |o| o.lamps.len() as u64).max(1);
            wired.turn_placement(o, (rng.next() % n) as i64);
        }
        Self::wired(wired.effective_patch().clone(), wired_as)
    }

    /// Where one of an object's lamps really is.
    pub fn landing(&self, object: &TreeRef, lamp: u32) -> Option<Landing> {
        let s = self
            .by_object
            .get(object)?
            .iter()
            .map(|&i| &self.truth.pixel_runs[i])
            .find(|s| s.lamps.contains(&lamp))?;
        Some(Landing {
            output: s.output.clone(),
            port: s.port.clone(),
            pixel: s.pixel(lamp),
        })
    }

    /// What a lamp of the piece shows: the board's pixel it is really on.
    /// `None` when it is on none.
    pub fn lamp(&self, board: &BoardFrame, object: &TreeRef, lamp: u32) -> Option<Rgb> {
        let at = self.landing(object, lamp)?;
        board
            .port(&at.output, &at.port)?
            .get(at.pixel as usize)
            .copied()
    }

    /// Every object the project's patch puts somewhere the piece doesn't,
    /// in the project's tree order.
    pub fn differences(&self, project: &Project) -> Vec<Difference> {
        project
            .placed_segments()
            .iter()
            .filter_map(|(_, o)| self.difference(project, &o.id))
            .collect()
    }

    /// How one object's place differs from the piece's, if it does.
    pub fn difference(&self, project: &Project, object: &TreeRef) -> Option<Difference> {
        let patch = project.effective_patch();
        let n = project.segment(object)?.lamps.len() as u32;
        let truth: Vec<Option<Landing>> = (0..n).map(|j| self.landing(object, j)).collect();
        let now: Vec<Option<Landing>> = (0..n).map(|j| patch.landing(object, j)).collect();
        if truth == now {
            return None;
        }
        let kind = if now.iter().all(Option::is_none) {
            DifferenceKind::NotPatched
        } else if as_set(&truth) != as_set(&now) {
            DifferenceKind::OtherPlace
        } else if direction(&truth) != direction(&now) {
            DifferenceKind::Reversed
        } else {
            DifferenceKind::Turned
        };
        Some(Difference {
            object: object.clone(),
            kind,
        })
    }

    /// Whether the project's patch puts every lamp where the piece has it.
    pub fn matches(&self, project: &Project) -> bool {
        self.differences(project).is_empty()
    }
}

/// A small seeded generator (Knuth's MMIX constants): the model takes no
/// randomness from anywhere else (sans-IO).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

fn as_set(landings: &[Option<Landing>]) -> HashSet<&Landing> {
    landings.iter().flatten().collect()
}

/// Which way most of a run's neighbouring lamps step along their port:
/// up (true) or down.
fn direction(landings: &[Option<Landing>]) -> bool {
    let mut up = 0i64;
    for w in landings.windows(2) {
        if let (Some(a), Some(b)) = (&w[0], &w[1])
            && a.output == b.output
            && a.port == b.port
        {
            match b.pixel as i64 - a.pixel as i64 {
                1 => up += 1,
                -1 => up -= 1,
                _ => {}
            }
        }
    }
    up >= 0
}
