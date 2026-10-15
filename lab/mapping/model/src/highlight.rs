//! Highlights: what the selection lights up across the patch, both ways.
//!
//! Select a port and the lamps it carries light up; select an object, a
//! fixture or a module and the ports it lands on light up. Select a pixel
//! and the lamp patched to it is ringed on the canvas; select a lamp and
//! its pixel is ringed on its port (D47). All of it is read off the
//! effective patch, so it is always what the patch really does.

use crate::editor::Editor;
use crate::node::NodeId;
use crate::object::ObjectId;
use crate::patch::Span;
use crate::project::Project;
use crate::target::Target;

impl Project {
    /// Every lamp one port carries, as (object, lamp index), in pixel order.
    pub fn lamps_on_port(&self, output: &NodeId, key: &str) -> Vec<(ObjectId, u32)> {
        self.effective_patch()
            .spans
            .iter()
            .filter(|s| &s.output == output && s.port == key)
            .flat_map(|s| s.lamps.clone().map(move |i| (s.object.clone(), i)))
            .collect()
    }

    /// The ports a target's lamps land on, in patch order. A port or an
    /// output lands on nothing.
    pub fn ports_of(&self, t: &Target) -> Vec<Target> {
        let mut out: Vec<Target> = Vec::new();
        for s in self.effective_patch().spans {
            if !self.span_belongs_to(&s, t) {
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
        for s in self.effective_patch().spans {
            if &s.fixture == fixture && !out.contains(&s.output) {
                out.push(s.output);
            }
        }
        out
    }

    /// The pixel a lamp lands on, if it is patched.
    pub fn pixel_of_lamp(&self, object: &ObjectId, index: u32) -> Option<Target> {
        self.effective_patch()
            .spans
            .into_iter()
            .find(|s| &s.object == object && s.lamps.contains(&index))
            .map(|s| Target::Pixel {
                index: s.start_pixel + index - s.lamps.start,
                output: s.output,
                key: s.port,
            })
    }

    /// The lamp patched to a pixel, if any.
    pub fn lamp_on_pixel(&self, output: &NodeId, key: &str, index: u32) -> Option<Target> {
        self.effective_patch()
            .spans
            .into_iter()
            .find(|s| {
                &s.output == output
                    && s.port == key
                    && (s.start_pixel..s.start_pixel + s.lamps.len() as u32).contains(&index)
            })
            .map(|s| Target::Lamp {
                index: s.lamps.start + index - s.start_pixel,
                object: s.object,
            })
    }

    fn span_belongs_to(&self, s: &Span, t: &Target) -> bool {
        match t {
            Target::Lamp { object, index } => &s.object == object && s.lamps.contains(index),
            Target::Port { .. } | Target::Pixel { .. } => false,
            Target::Node(id) if self.output(id).is_some() => false,
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
