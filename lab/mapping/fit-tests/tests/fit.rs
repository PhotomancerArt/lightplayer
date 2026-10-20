//! The fit: each test project, opened by the lab and by the product, says
//! the same thing (fit-check §6). Three proofs per project:
//!
//! 1. **Files.** Saving an unedited project writes nothing, so every file
//!    stays byte for byte; and every file the lab *would* write (all of
//!    them, forced) reads back through the product's own readers
//!    (`Map2dDoc::from_json`, `PatchDoc::from_json`, `NodeDef` and back out
//!    of its canonical writer) as exactly what the original reads as.
//! 2. **Lamps.** Every lamp the lab resolves is, bit for bit, the lamp
//!    `lpc_mapping::resolve` gives — position and order — and its strands
//!    are the product's (`/sector/2` and its lamp range).
//! 3. **Wire.** For every output, each wire lamp carries the same fixture
//!    lamp in the lab's effective patch as in the product's engine, loaded
//!    headless from the same files and ticked (its published fragments,
//!    with the product's reverse-then-rotate arithmetic).
//!
//! The projects: every project in `catalog/projects/`, every pattern in
//! `catalog/patterns/` and every test project in `projects/test/` (plan
//! M4: the Graph opens everything Map opens), and the radiance dome
//! (generated, placeholder counts).
//!
//! ```bash
//! cargo test -p lab-mapping-fit-tests
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use lab_mapping_files::{DirFiles, Opened};
use lab_mapping_model::{ComponentKind, NodeId, NodeKind, ObjectId, PatchRule, Project};
use lpc_engine::{ButtonService, EngineServices, ProjectLoader, RadioService};
use lpc_hardware::{HardwareSystem, HwRegistry, default_esp32c6_hardware_manifest};
use lpc_mapping::{Map2dDoc, PatchDoc, object_instance_spans, resolve};
use lpc_model::{NodeDef, SlotShapeRegistry, TreePath};
use lpfs::LpFsStd;

// ---- the projects ------------------------------------------------------------

macro_rules! fit {
    ($project:ident, $dir:expr) => {
        fit!($project, $dir, wire);
    };
    // `no_wire, "why"`: a project the engine never places a lamp for, so
    // the third proof has nothing to compare (the reason is the record).
    ($project:ident, $dir:expr, no_wire, $why:literal) => {
        fit!(@files_and_lamps $project, $dir);
    };
    ($project:ident, $dir:expr, wire) => {
        fit!(@files_and_lamps $project, $dir, {
            #[test]
            fn every_output_carries_the_same_lamps() {
                super::every_output_carries_the_same_lamps(&$dir);
            }
        });
    };
    (@files_and_lamps $project:ident, $dir:expr $(, { $($wire:tt)* })?) => {
        mod $project {
            use super::*;

            #[test]
            fn untouched_files_stay_byte_for_byte() {
                super::untouched_files_stay_byte_for_byte(&$dir);
            }

            #[test]
            fn written_files_read_the_same_in_the_product() {
                super::written_files_read_the_same_in_the_product(&$dir);
            }

            #[test]
            fn lamps_are_the_products() {
                super::lamps_are_the_products(&$dir);
            }

            $($($wire)*)?
        }
    };
}

fit!(radiance_dome, radiance());
fit!(whole_shapes, lab_project("whole-shapes"));
fit!(eaves, lab_project("eaves"));

// catalog/projects/
fit!(fiber_headband, catalog("fiber-headband"));
fit!(fyeah_sign, catalog("fyeah-sign"));
fit!(logo_sign, catalog("logo-sign"));
fit!(peach_1d, catalog("peach-1d"));
fit!(peach_2d, catalog("peach-2d"));
fit!(playful_choker, catalog("playful-choker"));
fit!(playful_choker_tryout, catalog("playful-choker-tryout"));
fit!(rocaille, catalog("rocaille"));
fit!(small_dome, catalog("small-dome"));
fit!(zook_dome, catalog("zook-dome"));

// catalog/patterns/
fit!(pattern_aurora, pattern("aurora"));
fit!(pattern_color_wipe, pattern("color-wipe"));
fit!(pattern_comet, pattern("comet"));
fit!(pattern_fire2012, pattern("fire2012"));
fit!(pattern_fireflies, pattern("fireflies"));
fit!(pattern_heartbeat, pattern("heartbeat"));
fit!(pattern_linear_gradient, pattern("linear-gradient"));
fit!(pattern_meteor, pattern("meteor"));
fit!(pattern_noise_hard, pattern("noise-hard"));
fit!(pattern_noise_soft, pattern("noise-soft"));
fit!(pattern_palette_waves, pattern("palette-waves"));
fit!(pattern_plasma, pattern("plasma"));
fit!(pattern_plasma_duo, pattern("plasma-duo"));
fit!(pattern_pulse, pattern("pulse"));
fit!(pattern_radial_gradient, pattern("radial-gradient"));
fit!(pattern_ripples, pattern("ripples"));
fit!(pattern_scanner, pattern("scanner"));
fit!(pattern_spiral, pattern("spiral"));
fit!(pattern_twinkle, pattern("twinkle"));
fit!(pattern_veins, pattern("veins"));

// projects/test/
fit!(test_basic, test_project("basic"));
fit!(test_basic_2n, test_project("basic-2n"));
fit!(test_basic_2n_half, test_project("basic-2n-half"));
fit!(test_basic_4n, test_project("basic-4n"));
fit!(test_basic2, test_project("basic2"));
fit!(test_button, test_project("button"));
fit!(test_button_playlist, test_project("button-playlist"));
fit!(test_button_sign, test_project("button-sign"));
fit!(test_events, test_project("events"));
fit!(test_fast, test_project("fast"));
fit!(
    test_fault_demo,
    test_project("fault-demo"),
    no_wire,
    "its shader faults every frame on purpose: the output shows the fault, never the lamps"
);
fit!(test_five_wire, test_project("five-wire"));
fit!(test_fluid, test_project("fluid"));
fit!(test_fyeah_button, test_project("fyeah-button"));
fit!(test_fyeah_sign, test_project("fyeah-sign"));
fit!(test_penta_strands_v3, test_project("penta-strands-v3"));
fit!(test_perf_baseline, test_project("perf/baseline"));
fit!(test_perf_fastmath, test_project("perf/fastmath"));
fit!(test_quad_equal100_v3, test_project("quad-equal100-v3"));
fit!(test_quad_gamma_full, test_project("quad-gamma-full"));
fit!(test_quad_gamma_v3, test_project("quad-gamma-v3"));
fit!(test_quad_strips, test_project("quad-strips"));
fit!(test_quad_strips_1fix, test_project("quad-strips-1fix"));
fit!(test_quad_strips_v3, test_project("quad-strips-v3"));
fit!(test_quad_wire_oracle, test_project("quad-wire-oracle"));
fit!(test_quad60_v3, test_project("quad60-v3"));
fit!(test_shader_oracle, test_project("shader-oracle"));
fit!(test_shader_oracle_f32, test_project("shader-oracle-f32"));
fit!(test_zook_dome_1500, test_project("zook-dome-1500"));

/// The radiance dome loads in the product as specified: 275 panels × 80
/// lamps = 22,000 lamps, 10 outputs × 32 ports, every panel patched once.
#[test]
fn the_radiance_dome_loads_in_the_product_as_specified() {
    let dir = radiance();
    let doc = Map2dDoc::from_json(&read(&dir.join("dome.map2d.json"))).unwrap();
    let lamps = resolve(&doc).unwrap();
    assert_eq!(doc.objects.len(), 55);
    assert_eq!(lamps.lamps.len(), 22_000);
    assert_eq!(lamps.spans.len(), 275);
    assert!(lamps.spans.iter().all(|s| s.count == 80));
    let mut ports = 0;
    for k in 1..=10 {
        let def = NodeDef::from_json_str(&read(&dir.join(format!("out_{k}.json")))).unwrap();
        let NodeDef::Output(o) = def else {
            panic!("out_{k} is an output");
        };
        assert_eq!(o.ports.entries.len(), 32, "out_{k}");
        ports += o.ports.entries.len();
    }
    assert_eq!(ports, 320);
    let patch = PatchDoc::from_json(&read(&dir.join("dome.patch.json"))).unwrap();
    assert_eq!(patch.entries.len(), 275);
    // And the engine runs it: 275 runs on the wire, 22,000 lamps.
    let wire: usize = engine_wire(&dir).values().map(BTreeMap::len).sum();
    assert_eq!(wire, 22_000);
}

// ---- edited, then saved ----------------------------------------------------

/// Edits the lab saves still fit: the zook dome with one copy fewer lamps
/// and copy 3 placed by hand at the start of port 4, reversed and turned
/// (the rest flow on after it, as the product reflows them).
#[test]
fn an_edited_zook_dome_still_fits() {
    edited_still_fits(&catalog("zook-dome"), |p| {
        let fixture = NodeId::new("fixture");
        let sector = p.fixture(&fixture).unwrap().components[0].id.clone();
        if let ComponentKind::Repeat { shape, .. } = &mut p.component_mut(&sector).unwrap().kind
            && let ComponentKind::Path { lamps, .. } = &mut **shape
        {
            *lamps = 290;
        }
        p.fixture_mut(&fixture).unwrap().rules.insert(
            0,
            PatchRule::Place {
                object: ObjectId::new(&sector, "3"),
                output: NodeId::new("output"),
                port: "4".into(),
                pixel: 0,
                reversed: true,
                rotation: 7,
            },
        );
    });
}

/// The choker with one letter placed by hand far down its one port,
/// reversed and turned, and its colour order changed.
#[test]
fn an_edited_choker_still_fits() {
    edited_still_fits(&catalog("playful-choker"), |p| {
        let fixture = NodeId::new("fixture");
        let a = p.fixture(&fixture).unwrap().components[2].id.clone();
        let f = p.fixture_mut(&fixture).unwrap();
        f.color_order = lab_mapping_model::ColorOrder::Grb;
        f.rules.insert(
            0,
            PatchRule::Place {
                object: ObjectId::new(&a, "path"),
                output: NodeId::new("output"),
                port: "0".into(),
                pixel: 90,
                reversed: true,
                rotation: 2,
            },
        );
    });
}

/// The radiance dome with one panel's run flipped and turned, another
/// panel unpatched (dark: the patch is manual), and a third moved to
/// output 10.
#[test]
fn an_edited_radiance_dome_still_fits() {
    edited_still_fits(&radiance(), |p| {
        let rules = &mut p.fixture_mut(&NodeId::new("dome")).unwrap().rules;
        if let PatchRule::Place {
            reversed, rotation, ..
        } = &mut rules[10]
        {
            *reversed = !*reversed;
            *rotation = 13;
        }
        rules.remove(20);
        if let PatchRule::Place {
            output,
            port,
            pixel,
            ..
        } = &mut rules[30]
        {
            *output = NodeId::new("out_10");
            *port = "31".into();
            *pixel = 0;
        }
    });
}

/// The whole shapes, each edited from the inspector: the grid one column
/// wider and its routing flipped, the filled polygon's lattice finer and
/// turned, the ring set given more lamps and one more ring.
#[test]
fn edited_whole_shapes_still_fit() {
    edited_still_fits(&lab_project("whole-shapes"), |p| {
        let ids: Vec<_> = p
            .fixture(&NodeId::new("fixture"))
            .unwrap()
            .components
            .iter()
            .map(|c| c.id.clone())
            .collect();
        let edits: [(&str, lab_mapping_model::PropEdit); 7] = [
            ("columns", lab_mapping_model::PropEdit::SetCount(6)),
            ("snake", lab_mapping_model::PropEdit::SetFlag(true)),
            ("pitch", lab_mapping_model::PropEdit::SetNumber(3.0)),
            ("angle", lab_mapping_model::PropEdit::SetNumber(-20.0)),
            ("lamps", lab_mapping_model::PropEdit::SetCount(36)),
            ("rings", lab_mapping_model::PropEdit::SetCount(4)),
            ("inner-first", lab_mapping_model::PropEdit::SetFlag(false)),
        ];
        let owner = [0, 0, 1, 1, 2, 2, 2];
        for ((prop, edit), i) in edits.into_iter().zip(owner) {
            assert!(
                p.edit_prop(
                    &lab_mapping_model::Target::Component(ids[i].clone()),
                    prop,
                    edit,
                    2.0
                ),
                "{prop}"
            );
        }
    });
}

/// The plasma pattern's disc with one more lamp on its outer ring: the
/// ring has a count of its own, so that is what the edit writes.
#[test]
fn an_edited_pattern_disc_still_fits() {
    edited_still_fits(&pattern("plasma"), |p| {
        let disc = p.fixture(&NodeId::new("fixture")).unwrap().components[1]
            .id
            .clone();
        assert!(p.edit_prop(
            &lab_mapping_model::Target::Component(disc),
            "lamps",
            lab_mapping_model::PropEdit::Nudge(1),
            2.0
        ));
        assert_eq!(p.lamp_count(), 242);
    });
}

fn edited_still_fits(dir: &Path, edit: impl Fn(&mut Project)) {
    let copy = scratch_copy(dir, "edited");
    let opened = open(&copy);
    let mut p = opened.project.clone();
    edit(&mut p);
    let saved = opened.files.save(&p).unwrap();
    assert!(!saved.writes.is_empty());
    for (path, text) in &saved.writes {
        std::fs::write(copy.join(path), text).unwrap();
    }
    for path in &saved.removed {
        let _ = std::fs::remove_file(copy.join(path));
    }
    // It opens again as the project the lab saved…
    assert_eq!(open(&copy).project, p);
    // …and the product agrees with it, lamp for lamp and on every wire.
    lamps_are_the_products(&copy);
    every_output_carries_the_same_lamps(&copy);
}

// ---- 1. files ------------------------------------------------------------------

fn untouched_files_stay_byte_for_byte(dir: &Path) {
    let copy = scratch_copy(dir, "untouched");
    let opened = open(&copy);
    let saved = opened.files.save(&opened.project).unwrap();
    assert!(
        saved.writes.is_empty(),
        "an unedited save wrote {:?}",
        paths(&saved.writes)
    );
    assert!(saved.removed.is_empty());
    for (path, text) in &saved.writes {
        std::fs::write(copy.join(path), text).unwrap();
    }
    for f in files_under(dir) {
        let rel = f.strip_prefix(dir).unwrap();
        assert_eq!(
            std::fs::read(&f).unwrap(),
            std::fs::read(copy.join(rel)).unwrap(),
            "{} changed",
            rel.display()
        );
    }
}

fn written_files_read_the_same_in_the_product(dir: &Path) {
    let opened = open(dir);
    let saved = opened.files.save_all(&opened.project).unwrap();
    assert!(!saved.writes.is_empty());
    for (path, text) in &saved.writes {
        let original = read(&dir.join(path));
        if path.ends_with(".map2d.json") {
            // A written mapping gives every object an id: the very ids the
            // product mints for a document with none (`ensure_object_ids`),
            // at the lowest format that holds them.
            let mut expected = Map2dDoc::from_json(&original).unwrap();
            lpc_mapping::ensure_object_ids(&mut expected);
            expected.format = expected.required_format();
            assert_eq!(Map2dDoc::from_json(text).unwrap(), expected, "{path}");
        } else if path.ends_with(".patch.json") {
            assert_eq!(
                PatchDoc::from_json(text).unwrap(),
                PatchDoc::from_json(&original).unwrap(),
                "{path}"
            );
        } else {
            // Through the product's reader and back out of its canonical
            // writer (a def carries slot revisions, which never compare).
            let canonical = |t: &str| {
                NodeDef::from_json_str(t)
                    .unwrap()
                    .write_json(&SlotShapeRegistry::default())
                    .unwrap()
            };
            assert_eq!(canonical(text), canonical(&original), "{path}");
        }
    }
}

// ---- 2. lamps ------------------------------------------------------------------

fn lamps_are_the_products(dir: &Path) {
    let opened = open(dir);
    let p = &opened.project;
    let mut fixtures = 0;
    for n in p.fixtures() {
        // A fixture the lab opens as its box alone has no lamps to compare
        // (`the_fixtures_the_graph_cant_show_yet` names them).
        if n.fixture().is_some_and(|f| f.unsupported.is_some()) {
            fixtures += 1;
            continue;
        }
        let map = opened.files.nodes[&n.id].map.as_ref().unwrap();
        let doc = Map2dDoc::from_json(&read(&dir.join(&map.path))).unwrap();
        let product = resolve(&doc).unwrap();
        let strands = object_instance_spans(&doc, &product);

        // Positions, in wiring order, bit for bit.
        let lab: Vec<[f32; 2]> = lab_objects(p, &n.id)
            .iter()
            .flat_map(|(_, o)| o.lamps.iter().map(|l| [l.pos.x as f32, l.pos.y as f32]))
            .collect();
        let theirs = product.positions();
        assert_eq!(lab.len(), theirs.len(), "{}: lamp count", n.name);
        for (i, (a, b)) in lab.iter().zip(&theirs).enumerate() {
            assert!(
                a[0].to_bits() == b[0].to_bits() && a[1].to_bits() == b[1].to_bits(),
                "{} lamp {i}: lab {a:?}, product {b:?}",
                n.name
            );
        }

        // Strands: the same objects and copies, the same lamps.
        let mut start = 0u32;
        let mut lab_strands = Vec::new();
        for (path, o) in lab_objects(p, &n.id) {
            let count = o.lamps.len() as u32;
            lab_strands.push((path, start, count));
            start += count;
        }
        let product_strands: Vec<(String, u32, u32)> = strands
            .iter()
            .map(|s| {
                let mut path = format!("/{}", s.id.as_ref().map_or("?", |i| i.as_str()));
                for k in &s.instances {
                    path.push_str(&format!("/{k}"));
                }
                (path, s.start, s.count)
            })
            .collect();
        // Documents without ids: the product can't name a strand, so only
        // the lamp ranges compare.
        if strands.iter().all(|s| s.id.is_some()) {
            assert_eq!(lab_strands, product_strands, "{}", n.name);
        } else {
            let ranges = |v: &[(String, u32, u32)]| -> Vec<(u32, u32)> {
                v.iter().map(|(_, s, c)| (*s, *c)).collect()
            };
            assert_eq!(ranges(&lab_strands), ranges(&product_strands), "{}", n.name);
        }
        fixtures += 1;
    }
    assert!(fixtures > 0);
}

/// A fixture's objects in wiring order, in its own coordinates, each with
/// the product path that names it.
/// The fixtures the Graph opens as their box alone, and why: those mapped
/// with the product's older inline point lists instead of a map2d document
/// (Map can't open them either). Every other fixture in every project
/// opens whole (plan M4), and a project here still opens round the box.
#[test]
fn the_fixtures_the_graph_cant_show_yet() {
    let mut boxes = Vec::new();
    for dir in every_project() {
        let opened = open(&dir);
        for n in opened.project.fixtures() {
            if let Some(u) = n.fixture().and_then(|f| f.unsupported.as_ref()) {
                let name = dir.strip_prefix(workspace()).unwrap().display().to_string();
                boxes.push(format!("{name} {}: {} ({})", n.name, u.what, u.edit_in));
            }
        }
    }
    assert_eq!(
        boxes,
        [
            "catalog/projects/fiber-headband fixture: its point-list mapping (Nodes)",
            "projects/test/five-wire fixture: its point-list mapping (Nodes)",
            "projects/test/penta-strands-v3 fixture: its point-list mapping (Nodes)",
        ]
    );
}

/// Every project the oracle reads, in path order.
fn every_project() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in ["catalog/projects", "catalog/patterns", "projects/test"] {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(workspace().join(root))
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        for d in dirs {
            if d.join("project.json").exists() {
                out.push(d);
            } else {
                // A folder of projects (perf/baseline, perf/fastmath).
                let mut inner: Vec<PathBuf> = std::fs::read_dir(&d)
                    .unwrap()
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.join("project.json").exists())
                    .collect();
                inner.sort();
                out.extend(inner);
            }
        }
    }
    out
}

fn lab_objects(p: &Project, fixture: &NodeId) -> Vec<(String, lab_mapping_model::Object)> {
    let f = p.fixture(fixture).unwrap();
    let mut out = Vec::new();
    for c in &f.components {
        for o in lab_mapping_model::domain::object::objects_of(c) {
            let path = match &c.kind {
                ComponentKind::Repeat { .. } => format!("/{}/{}", c.id, o.id.key),
                _ => format!("/{}", c.id),
            };
            out.push((path, o));
        }
    }
    out
}

// ---- 3. wire -------------------------------------------------------------------

/// Per output (by its path in the tree), wire lamp → (fixture, fixture lamp).
type Wire = BTreeMap<String, BTreeMap<u32, (String, u32)>>;

fn every_output_carries_the_same_lamps(dir: &Path) {
    let opened = open(dir);
    let lab = lab_wire(&opened);
    // The lamps of a fixture the lab opens as its box alone aren't the
    // lab's to place; every other lamp on every wire still is.
    let boxes: Vec<String> = opened
        .project
        .fixtures()
        .iter()
        .filter(|n| n.fixture().is_some_and(|f| f.unsupported.is_some()))
        .map(|n| tree_key(&opened, &n.id))
        .collect();
    let mut engine = engine_wire(dir);
    for lamps in engine.values_mut() {
        lamps.retain(|_, (fixture, _)| !boxes.contains(fixture));
    }
    engine.retain(|_, lamps| !lamps.is_empty());
    assert_eq!(
        lab.keys().collect::<Vec<_>>(),
        engine.keys().collect::<Vec<_>>(),
        "the outputs carrying lamps"
    );
    for (output, theirs) in &engine {
        let ours = &lab[output];
        assert_eq!(ours.len(), theirs.len(), "{output}: lamps on the wire");
        for (wire, lamp) in theirs {
            assert_eq!(ours.get(wire), Some(lamp), "{output} wire lamp {wire}");
        }
    }
}

fn lab_wire(opened: &Opened) -> Wire {
    let p = &opened.project;
    let mut starts: BTreeMap<(NodeId, lab_mapping_model::ObjectId), u32> = BTreeMap::new();
    for n in p.fixtures() {
        let mut start = 0;
        for (_, o) in lab_objects(p, &n.id) {
            let count = o.lamps.len() as u32;
            starts.insert((n.id.clone(), o.id), start);
            start += count;
        }
    }
    let mut wire = Wire::new();
    for s in &p.effective_patch().spans {
        let out = p.output(&s.output).unwrap();
        let port_start = out.port_start(&s.port).unwrap();
        let first = starts[&(s.fixture.clone(), s.object.clone())];
        for lamp in s.lamps.clone() {
            wire.entry(tree_key(opened, &s.output)).or_default().insert(
                port_start + s.pixel(lamp),
                (tree_key(opened, &s.fixture), first + lamp),
            );
        }
    }
    wire
}

/// A node as the engine's tree names it: `dome.module/dome.fixture`.
fn tree_key(opened: &Opened, id: &NodeId) -> String {
    let p = &opened.project;
    let mut chain = vec![id.clone()];
    while let Some(parent) = p.parent_module(chain.last().unwrap()) {
        chain.push(parent.clone());
    }
    chain
        .iter()
        .rev()
        .map(|n| {
            let kind = match p.node(n).map(|x| &x.kind) {
                Some(NodeKind::Module { .. }) => "module",
                Some(NodeKind::Fixture(_)) => "fixture",
                Some(NodeKind::Output(_)) => "output",
                _ => "node",
            };
            format!("{}.{kind}", opened.files.nodes[n].name)
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// The product's wire: the project loaded by the engine, ticked until its
/// placements settle, and each output's published fragments laid out lamp
/// by lamp.
fn engine_wire(dir: &Path) -> Wire {
    let fs = LpFsStd::new(dir.to_path_buf());
    let mut services = EngineServices::new(TreePath::parse("/fit.show").unwrap());
    // Virtual buttons and radios, so a project that has them (a sign with a
    // remote) loads and ticks headless.
    let hardware = Rc::new(HardwareSystem::with_virtual_drivers(Rc::new(
        HwRegistry::new(default_esp32c6_hardware_manifest()),
    )));
    let buttons: Rc<dyn ButtonService> = hardware.clone();
    let radios: Rc<dyn RadioService> = hardware;
    services.set_button_service(Some(buttons));
    services.set_radio_service(Some(radios));
    let mut rt = ProjectLoader::load_from_root(&fs, services).expect("the product loads it");
    rt.engine_mut()
        .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
    let (mut engine, registry) = rt.into_parts();
    // A frame a node faults in (a remote with no partner) is read anyway.
    for _ in 0..3 {
        let _ = engine.tick(&registry, 16);
    }
    let tree = engine.tree();
    let key = |path: &TreePath| -> String {
        let text = path.to_string();
        // Drop the root (`/fit.show/`).
        text.trim_start_matches('/')
            .split_once('/')
            .map_or(text.clone(), |(_, rest)| rest.to_string())
    };
    let mut wire = Wire::new();
    for entry in tree.entries() {
        let lpc_engine::node::NodeEntryState::Alive(node) = entry.state.value() else {
            continue;
        };
        for f in node.runtime_output_fragments() {
            let fixture = tree
                .get(f.product.node())
                .map(|e| key(&e.path))
                .unwrap_or_default();
            let count = f.len_samples / 3;
            for j in 0..count {
                let oriented = if f.reversed { count - 1 - j } else { j };
                let at = f.offset_samples / 3 + (oriented + f.rotation_samples / 3) % count.max(1);
                wire.entry(key(&entry.path))
                    .or_default()
                    .insert(at, (fixture.clone(), f.source_offset_samples / 3 + j));
            }
        }
    }
    wire
}

// ---- helpers -------------------------------------------------------------------

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn catalog(name: &str) -> PathBuf {
    workspace().join("catalog/projects").join(name)
}

fn pattern(name: &str) -> PathBuf {
    workspace().join("catalog/patterns").join(name)
}

fn test_project(name: &str) -> PathBuf {
    workspace().join("projects/test").join(name)
}

fn lab_project(name: &str) -> PathBuf {
    workspace().join("lab/mapping/projects").join(name)
}

fn radiance() -> PathBuf {
    workspace().join("lab/mapping/projects/radiance-dome")
}

fn open(dir: &Path) -> Opened {
    Opened::open(&DirFiles::new(dir)).unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn paths(writes: &[(String, String)]) -> Vec<&str> {
    writes.iter().map(|(p, _)| p.as_str()).collect()
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let path = e.unwrap().path();
        if path.is_dir() {
            out.extend(files_under(&path));
        } else {
            out.push(path);
        }
    }
    out.sort();
    out
}

/// A fresh copy of a project, to save into.
fn scratch_copy(dir: &Path, why: &str) -> PathBuf {
    let name = dir.file_name().unwrap().to_string_lossy();
    let to = std::env::temp_dir().join(format!(
        "lab-mapping-fit-{}-{name}-{why}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&to);
    for f in files_under(dir) {
        let rel = f.strip_prefix(dir).unwrap();
        let dest = to.join(rel);
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::copy(&f, dest).unwrap();
    }
    to
}
