//! Highlights: what the selection lights up across the patch, both ways.
//!
//! Select a port and the lamps it carries light up; select an object, a
//! fixture or a module and the ports it lands on light up. Select a pixel
//! and the lamp patched to it is ringed on the canvas; select a lamp and
//! its pixel is ringed on its port (D47). All of it is read off the
//! effective patch, so it is always what the patch really does.

use crate::domain::node::NodeId;
use crate::domain::object::ObjectId;
use crate::domain::patch::PixelRun;
use crate::domain::project::Project;
use crate::domain::target::Target;
use crate::ux::editor::Editor;

impl Project {
    /// Every lamp one port carries, as (object, lamp index), in pixel order.
    pub fn lamps_on_port(&self, output: &NodeId, key: &str) -> Vec<(ObjectId, u32)> {
        let mut on: Vec<(u32, ObjectId, u32)> = self
            .effective_patch()
            .pixel_runs
            .iter()
            .filter(|s| &s.output == output && s.port == key)
            .flat_map(|s| {
                s.lamps
                    .clone()
                    .map(move |i| (s.pixel(i), s.object.clone(), i))
            })
            .collect();
        on.sort_by_key(|(pixel, _, _)| *pixel);
        on.into_iter().map(|(_, o, i)| (o, i)).collect()
    }

    /// The ports a target's lamps land on, in patch order. A port or an
    /// output lands on nothing.
    pub fn ports_of(&self, t: &Target) -> Vec<Target> {
        let mut out: Vec<Target> = Vec::new();
        for s in &self.effective_patch().pixel_runs {
            if !self.span_belongs_to(s, t) {
                continue;
            }
            let port = Target::Port {
                output: s.output.clone(),
                key: s.port.clone(),
            };
            if !out.contains(&port) {
                out.push(port);
            }
        }
        out
    }

    /// The outputs a fixture's lamps land on, in patch order.
    pub fn outputs_of(&self, fixture: &NodeId) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = Vec::new();
        for s in &self.effective_patch().pixel_runs {
            if &s.fixture == fixture && !out.contains(&s.output) {
                out.push(s.output.clone());
            }
        }
        out
    }

    /// The pixel a lamp lands on, if it is patched.
    pub fn pixel_of_lamp(&self, object: &ObjectId, index: u32) -> Option<Target> {
        self.effective_patch()
            .pixel_runs
            .iter()
            .find(|s| &s.object == object && s.lamps.contains(&index))
            .map(|s| Target::Pixel {
                index: s.pixel(index),
                output: s.output.clone(),
                key: s.port.clone(),
            })
    }

    /// The lamp patched to a pixel, if any.
    pub fn lamp_on_pixel(&self, output: &NodeId, key: &str, index: u32) -> Option<Target> {
        self.effective_patch()
            .pixel_runs
            .iter()
            .filter(|s| &s.output == output && s.port == key)
            .find_map(|s| {
                Some(Target::Lamp {
                    index: s.lamp_at(index)?,
                    object: s.object.clone(),
                })
            })
    }

    fn span_belongs_to(&self, s: &PixelRun, t: &Target) -> bool {
        match t {
            Target::Lamp { object, index } => &s.object == object && s.lamps.contains(index),
            Target::Port { .. } | Target::Span { .. } | Target::Pixel { .. } => false,
            Target::Node(id) if self.output(id).is_some() => false,
            // A fixture: its own spans, without walking up from each one.
            Target::Node(id) if self.fixture(id).is_some() => &s.fixture == id,
            // The span's object, and everything above it up to the root.
            _ => self.is_ancestor_or_self(t, &self.object_target(&s.object)),
        }
    }
}

impl Editor {
    /// The lamps ringed on the canvas: every lamp a selected port carries,
    /// and the lamp on each selected pixel.
    pub fn marked_lamps(&self) -> Vec<Target> {
        let p = &self.project;
        let mut out: Vec<Target> = Vec::new();
        for t in &self.selection {
            let lamps = match t {
                Target::Port { output, key } => p
                    .lamps_on_port(output, key)
                    .into_iter()
                    .map(|(object, index)| Target::Lamp { object, index })
                    .collect(),
                Target::Pixel { output, key, index } => {
                    p.lamp_on_pixel(output, key, *index).into_iter().collect()
                }
                // A span marks the lamps of its object that land on it.
                Target::Span { output, key, .. } => match p.span_of(t) {
                    Some(s) => s
                        .pixels
                        .filter_map(|px| p.lamp_on_pixel(output, key, px))
                        .collect(),
                    None => Vec::new(),
                },
                _ => Vec::new(),
            };
            for l in lamps {
                if !out.contains(&l) {
                    out.push(l);
                }
            }
        }
        out
    }

    /// The pixels ringed on the port rows: the pixel of each selected lamp.
    pub fn marked_pixels(&self) -> Vec<Target> {
        let mut out: Vec<Target> = Vec::new();
        for t in &self.selection {
            if let Target::Lamp { object, index } = t
                && let Some(px) = self.project.pixel_of_lamp(object, *index)
                && !out.contains(&px)
            {
                out.push(px);
            }
        }
        out
    }
}
