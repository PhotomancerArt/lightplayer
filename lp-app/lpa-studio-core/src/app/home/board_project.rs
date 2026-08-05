//! The board-aware first project: what "new project for this board" means.
//!
//! The setup flow (`planning/…/flow-spec.md` §F3) promises one compact
//! line — *meteor → 256-px strip → \<pin\>* — and this module is that line
//! made real. Given a catalog board id it produces a complete package:
//!
//! ```text
//! clock ──bus:time──▶ playlist ──bus:visual.out──▶ fixture ──bus:control.out──▶ output
//!                        └── entry 1: effect/ (the meteor module, vendored)
//! ```
//!
//! Three deliberate shapes:
//!
//! - **The effect is a vendored local sub-module** (`effect/`), not two
//!   loose nodes. Meteor is a compute/render *pair* wired to each other
//!   (`node:../sim#meteors`); keeping them inside one module folder keeps
//!   that wiring untouched and makes the entry playlist-playable through
//!   the module mirror (modules.md R7: "every module node produces an
//!   `output` slot mirroring its own scope's `visual.out` … this is what
//!   makes any module playlist-playable with zero playlist changes"). Its
//!   `bus:time` read resolves outward to the root clock by R5.
//! - **The playlist is authored even with one entry.** It is the seam the
//!   user adds their second effect to; a first project without one teaches
//!   the wrong shape.
//! - **One wire.** The endpoint is `ws281x:local:<board's first default
//!   LED wire>` (`lpa_boards::BoardDisplayFile::default_led_wire`).
//!   Boards with several wires (dig-uno, DOM-Z-102) generate onto the
//!   first and leave the rest unauthored — multi-output generation is
//!   future work, not a silent guess about how many strips are plugged in.
//!
//! The shader artifacts are **copied byte-for-byte** out of the embedded
//! meteor example, so what the gallery ships and what the wizard generates
//! cannot drift. The container files are authored here.

use lpa_boards::board_by_id;
use lpc_model::{HwEndpointSpec, ProjectManifest};

use super::embedded_example::METEOR_FILES;

/// Pixels on the generated fixture's strip. Modest on purpose: enough to
/// look like a strip rather than a token, few enough that a user who
/// plugged in less is only dark at the far end (settled at the P01 spike).
pub const DEFAULT_STRIP_PIXELS: u32 = 256;

/// Height, in fixture-texture rows, the strip samples the middle of.
/// The meteor render shader fades by `uv.y` distance from a head at
/// `y = 0.5`, so the strip row has to sit mid-texture to be lit at all.
const STRIP_TEXTURE_ROWS: u32 = 8;

/// A generated package, ready for `LibraryStore::install_package`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedProject {
    /// Human project name; also the label the library slug is dated from.
    pub name: String,
    /// The board this was generated for — the manifest's `target`.
    pub board_id: String,
    /// The authored output endpoint (`ws281x:local:<wire>`).
    pub endpoint: String,
    /// Package-relative files in deploy order (`project.json` first).
    pub files: Vec<(String, Vec<u8>)>,
}

/// Why a board could not be generated for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenerateProjectError {
    /// No display sidecar ships for this id.
    UnknownBoard { board_id: String },
    /// The board ships no `default_led_wires`. Refused rather than guessed:
    /// picking a pin for someone is the physical-damage class of mistake.
    NoDefaultWire { board_id: String },
    /// The board's wire name does not form an endpoint spec.
    InvalidWire { board_id: String, wire: String },
}

impl core::fmt::Display for GenerateProjectError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownBoard { board_id } => {
                write!(f, "unknown board {board_id}")
            }
            Self::NoDefaultWire { board_id } => write!(
                f,
                "board {board_id} declares no default LED wire — nothing to generate onto"
            ),
            Self::InvalidWire { board_id, wire } => {
                write!(f, "board {board_id} wire {wire:?} is not a valid endpoint")
            }
        }
    }
}

impl core::error::Error for GenerateProjectError {}

impl From<GenerateProjectError> for crate::UiError {
    fn from(error: GenerateProjectError) -> Self {
        crate::UiError::UnsupportedAction(error.to_string())
    }
}

/// Build the first project for `board_id`: clock → playlist(meteor) →
/// fixture → output, on the board's first default LED wire, with the
/// container manifest's `target` set to the board.
pub fn generate_board_project(board_id: &str) -> Result<GeneratedProject, GenerateProjectError> {
    let board = board_by_id(board_id).ok_or_else(|| GenerateProjectError::UnknownBoard {
        board_id: board_id.to_string(),
    })?;
    let wire = board
        .default_led_wire()
        .ok_or_else(|| GenerateProjectError::NoDefaultWire {
            board_id: board_id.to_string(),
        })?;
    let endpoint = HwEndpointSpec::parse(format!("ws281x:local:{wire}")).map_err(|_| {
        GenerateProjectError::InvalidWire {
            board_id: board_id.to_string(),
            wire: wire.to_string(),
        }
    })?;

    let name = board.display_name.clone();
    let manifest = ProjectManifest {
        name: Some(name.clone()),
        target: Some(board_id.to_string()),
        ..ProjectManifest::new_current(&name)
    };

    let mut files: Vec<(String, Vec<u8>)> = vec![
        (
            "project.json".to_string(),
            manifest.write_json().into_bytes(),
        ),
        ("module.json".to_string(), ROOT_MODULE.into()),
        ("clock.json".to_string(), CLOCK.into()),
        ("playlist.json".to_string(), PLAYLIST.into()),
        ("fixture.json".to_string(), fixture_json().into_bytes()),
        (
            "fixture.map2d.json".to_string(),
            strip_map2d_json().into_bytes(),
        ),
        (
            "output.json".to_string(),
            output_json(endpoint.as_str()).into_bytes(),
        ),
        ("effect/module.json".to_string(), EFFECT_MODULE.into()),
    ];
    // The effect's node artifacts and shaders ride verbatim from the
    // embedded example — the gallery's meteor and the generated one are
    // the same bytes by construction.
    for name in ["sim.json", "sim.glsl", "render.json", "render.glsl"] {
        files.push((format!("effect/{name}"), meteor_file(name).to_vec()));
    }

    Ok(GeneratedProject {
        name,
        board_id: board_id.to_string(),
        endpoint: endpoint.as_str().to_string(),
        files,
    })
}

/// One file of the embedded meteor example, by package-relative name.
fn meteor_file(name: &str) -> &'static [u8] {
    METEOR_FILES
        .iter()
        .find(|(path, _)| *path == name)
        .map(|(_, bytes)| *bytes)
        .unwrap_or_else(|| panic!("the embedded meteor example ships {name}"))
}

/// Root module: the four fixed nodes of a first project.
const ROOT_MODULE: &[u8] = br#"{
  "kind": "Module",
  "nodes": {
    "clock": {
      "ref": "./clock.json"
    },
    "playlist": {
      "ref": "./playlist.json"
    },
    "fixture": {
      "ref": "./fixture.json"
    },
    "output": {
      "ref": "./output.json"
    }
  }
}
"#;

const CLOCK: &[u8] = br#"{
  "kind": "Clock"
}
"#;

/// One entry, the vendored effect. `idle_entry` names it, so it plays with
/// no trigger wired — a first project animates the moment it loads.
const PLAYLIST: &[u8] = br#"{
  "kind": "Playlist",
  "bindings": {
    "time": {
      "source": "bus:time"
    }
  },
  "idle_entry": 1,
  "default_fade": 0.35,
  "entries": {
    "1": {
      "name": "meteor",
      "node": {
        "ref": "./effect/module.json"
      }
    }
  }
}
"#;

/// The vendored meteor module: the compute/render pair and nothing else
/// (the example's own clock/fixture/output are the host project's job).
/// Provenance is copied per modules.md R14 — vendoring keeps attribution.
const EFFECT_MODULE: &[u8] = br#"{
  "kind": "Module",
  "nodes": {
    "sim": {
      "ref": "./sim.json"
    },
    "render": {
      "ref": "./render.json"
    }
  },
  "provenance": {
    "author": "Photomancer",
    "version": "1",
    "license": "CC0-1.0"
  }
}
"#;

fn fixture_json() -> String {
    format!(
        r#"{{
  "kind": "Fixture",
  "render_size": {{
    "width": {DEFAULT_STRIP_PIXELS},
    "height": {STRIP_TEXTURE_ROWS}
  }},
  "bindings": {{
    "input": {{
      "source": "bus:visual.out"
    }},
    "output": {{
      "target": "bus:control.out"
    }}
  }},
  "sampling": "direct",
  "diagnostic_mode": "off",
  "mapping": {{
    "kind": "Map2d",
    "source": "fixture.map2d.json"
  }},
  "color_order": "rgb",
  "brightness": 1.0,
  "gamma_correction": false
}}
"#
    )
}

/// A one-row grid in a canvas the same shape as the fixture texture, so a
/// lamp is one texel wide and the row sits on the texture's centre line.
fn strip_map2d_json() -> String {
    let centre = f64::from(STRIP_TEXTURE_ROWS) / 2.0;
    format!(
        r#"{{
  "format": 1,
  "sample_diameter": 1.0,
  "canvas": [
    0.0,
    0.0,
    {DEFAULT_STRIP_PIXELS}.0,
    {STRIP_TEXTURE_ROWS}.0
  ],
  "objects": [
    {{
      "name": "strip",
      "shape": {{
        "grid": {{
          "origin": [
            0.5,
            {centre}
          ],
          "cols": {DEFAULT_STRIP_PIXELS},
          "rows": 1,
          "pitch": 1.0
        }}
      }}
    }}
  ]
}}
"#
    )
}

/// One channel, no count: the single-wire degenerate case of the channels
/// map — "this wire takes the whole control product" (the multi-endpoint
/// ADR §1). A second wire is authored later, in the editor.
fn output_json(endpoint: &str) -> String {
    format!(
        r#"{{
  "kind": "Output",
  "channels": {{
    "0": {{
      "endpoint": "{endpoint}"
    }}
  }},
  "bindings": {{
    "input": {{
      "source": "bus:control.out"
    }}
  }},
  "options": {{
    "white_point": [
      0.9,
      1,
      1
    ],
    "interpolation_enabled": true,
    "dithering_enabled": false,
    "lut_enabled": true
  }}
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_board_is_refused_not_guessed() {
        assert_eq!(
            generate_board_project("acme/not-a-board"),
            Err(GenerateProjectError::UnknownBoard {
                board_id: "acme/not-a-board".to_string()
            })
        );
    }

    #[test]
    fn the_generated_project_wires_the_boards_first_default_wire() {
        let project = generate_board_project("domraem/dom-z-102").expect("desk board");
        // The DOM-Z-102's four fused DATA terminals, first one taken.
        assert_eq!(project.endpoint, "ws281x:local:IO18");
        assert_eq!(project.board_id, "domraem/dom-z-102");
        let output = file_text(&project, "output.json");
        assert!(output.contains("\"ws281x:local:IO18\""), "{output}");
        assert!(
            !output.contains("IO16"),
            "single-wire generation authors ONE channel: {output}"
        );
    }

    #[test]
    fn the_manifest_carries_the_board_as_its_target() {
        let project = generate_board_project("seeed/xiao-esp32-c6").expect("xiao");
        let manifest =
            ProjectManifest::read_json(&file_text(&project, "project.json")).expect("manifest");
        assert_eq!(manifest.target.as_deref(), Some("seeed/xiao-esp32-c6"));
        assert_eq!(manifest.format, Some(lpc_model::PROJECT_FORMAT_VERSION));
        assert_eq!(manifest.name.as_deref(), Some("XIAO ESP32-C6"));
    }

    #[test]
    fn the_effect_files_are_the_embedded_examples_bytes() {
        let project = generate_board_project("seeed/xiao-esp32-c6").expect("xiao");
        for name in ["sim.glsl", "render.glsl", "sim.json", "render.json"] {
            let generated = file_bytes(&project, &format!("effect/{name}"));
            assert_eq!(
                generated,
                meteor_file(name),
                "effect/{name} must be the embedded meteor's own bytes"
            );
        }
    }

    #[test]
    fn the_container_manifest_deploys_first() {
        let project = generate_board_project("quinled/dig-uno").expect("dig-uno");
        assert_eq!(
            project.files.first().map(|(path, _)| path.as_str()),
            Some("project.json")
        );
        // The two container files a package is unopenable without.
        for required in ["project.json", "module.json"] {
            assert!(project.files.iter().any(|(path, _)| path == required));
        }
    }

    #[test]
    fn the_strip_is_the_default_pixel_count() {
        let project = generate_board_project("quinled/dig-uno").expect("dig-uno");
        let doc = lpc_mapping::Map2dDoc::from_json(&file_text(&project, "fixture.map2d.json"))
            .expect("the generated mapping parses");
        let resolved = lpc_mapping::resolve(&doc).expect("and resolves");
        assert_eq!(resolved.lamps.len(), DEFAULT_STRIP_PIXELS as usize);
    }

    fn file_bytes<'a>(project: &'a GeneratedProject, path: &str) -> &'a [u8] {
        project
            .files
            .iter()
            .find(|(name, _)| name == path)
            .map(|(_, bytes)| bytes.as_slice())
            .unwrap_or_else(|| panic!("generated package has {path}"))
    }

    fn file_text(project: &GeneratedProject, path: &str) -> String {
        String::from_utf8(file_bytes(project, path).to_vec()).expect("utf8")
    }
}
