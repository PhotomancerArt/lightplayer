//! Naming an output that has no name (lean Q96, C39).
//!
//! A fixture's start and its overrides name the output they are on (G1,
//! Q17), so an unnamed output is one nothing can patch to. Choosing a port
//! of one — the Port row, `set-port`, `set-fixture-start` — names it, in
//! the same undo step, with the lowest free number: `"1"`, `"2"`, …,
//! unique among the project's outputs. It is the rule Studio names every
//! output it creates with, and the format-12 upgrade too
//! (`lpc_model::next_output_name`, M3's Q59); this crate holds no
//! `lpc-model`, so the rule is said again here. Saving writes the name into
//! the output's own file (`lab-mapping-files`), with the mapping that names
//! it.

use lpc_mapping::patching::OutputName;

use crate::domain::node::NodeId;
use crate::domain::project::Project;

impl Project {
    /// The output's name on the wire, naming it first when it has none
    /// ([`Project::next_output_name`]).
    pub fn name_output(&mut self, output: &NodeId) -> Result<OutputName, String> {
        let current = self
            .output(output)
            .ok_or_else(|| format!("there is no output {output}"))?
            .label
            .clone();
        let name = match current {
            Some(name) => name,
            None => {
                let name = self.next_output_name();
                if let Some(o) = self.output_mut(output) {
                    o.label = Some(name.clone());
                }
                name
            }
        };
        Ok(OutputName::new(&name))
    }

    /// The name an unnamed output gets: the lowest number no output in the
    /// project carries.
    pub fn next_output_name(&self) -> String {
        let taken: Vec<&str> = self
            .outputs()
            .iter()
            .filter_map(|n| n.output()?.label.as_deref())
            .collect();
        (1u32..)
            .map(|n| n.to_string())
            .find(|name| !taken.contains(&name.as_str()))
            .expect("fewer outputs than numbers")
    }
}
