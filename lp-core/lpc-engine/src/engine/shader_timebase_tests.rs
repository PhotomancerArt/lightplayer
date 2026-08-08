//! Shader timebase uniforms end to end (TimeProduct M2 / P3).
//!
//! `phasor` and `seconds` slots are the first uniforms whose value does not
//! come from the slot's resolved data at all: the fill path resolves the
//! scope's `bus:time` to a [`TimeProduct`] handle and evaluates against the
//! engine's timebase store. What is at stake here is that whole seam —
//! provenance-derived phasor identity, config channels, live re-authoring,
//! and the loud failure when `bus:time` carries the wrong thing.
//!
//! **`bus:time` does not carry a product yet** (that swap is P4), so every
//! project here registers the product binding by hand — the same
//! `Literal → BusChannel("time")` shape the clock's produced slot will take
//! over. Everything downstream of that binding is the real path.
//!
//! Compute shaders rather than visual ones: their produced slots are
//! resolvable values, so a uniform can be read back as a number instead of
//! inferred from pixels. The fill path is shared with the visual node
//! (`resolve_or_default_input`), so this exercises both.
//!
//! One frame here is `tick()` (which advances engine time and bumps the
//! store's tick) followed by a `resolve` of the produced slot (which is what
//! actually demands the compute node, and through it the clock). Repeated
//! reads inside one frame see the same phase — advance-once-per-tick is the
//! store's contract, and these tests lean on it.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use lpc_model::{
    ChannelName, Kind, LpValue, NodeId, NodeRuntimeStatus, PhasorConfig, ProductRef, Revision,
    SlotPath, TimeProduct, ToLpValue, TreePath, Waveform,
};
use lpc_registry::{ParseCtx, ProjectRegistry};
use lpfs::{AsLpPath, FsEvent, FsEventKind, LpFs, LpFsMemory, LpPathBuf};

use crate::dataflow::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
use crate::dataflow::resolver::{QueryKey, ResolveLogLevel};
use crate::engine::{Engine, EngineServices, ProjectLoader, resolve_with_engine_host};

const TICK_MS: u32 = 100;
const TICK_SECONDS: f32 = 0.1;

// --- Harness ---------------------------------------------------------------

struct Project {
    engine: Engine,
    registry: ProjectRegistry,
    fs: LpFsMemory,
    revision: i64,
    /// Literal bus writers registered by hand. `apply_project_changes`
    /// rebuilds every binding from defs, so anything registered outside the
    /// loader has to be re-registered after an authoring edit — including
    /// the `bus:time` product binding that P4 will make part of the defs.
    literals: Vec<(LpValue, String, Kind)>,
}

impl Project {
    /// One frame: advance engine time and the store tick, then demand every
    /// compute node under test.
    fn frame(&mut self, nodes: &[NodeId]) {
        self.engine.tick(&self.registry, TICK_MS).expect("tick");
        for node in nodes {
            self.read(*node, "out_wave");
        }
    }

    /// The node whose tree path ends in `suffix`.
    fn node(&self, suffix: &str) -> NodeId {
        self.engine
            .tree()
            .entries()
            .find(|entry| entry.path.to_string().ends_with(suffix))
            .unwrap_or_else(|| panic!("no node ending in {suffix}"))
            .id
    }

    /// Resolve one of a compute node's produced slots, running the node.
    fn read(&mut self, node: NodeId, slot: &str) -> f32 {
        let (production, _) = resolve_with_engine_host(
            &mut self.engine,
            &self.registry,
            QueryKey::ProducedSlot {
                node,
                slot: SlotPath::parse(slot).expect("slot path"),
            },
            ResolveLogLevel::Off,
        )
        .unwrap_or_else(|e| panic!("resolve produced {slot:?}: {e}"));
        match production.value_leaf().expect("produced value").value() {
            LpValue::F32(value) => *value,
            other => panic!("produced slot {slot:?} is {other:?}"),
        }
    }

    /// Run the compute node until it is past the compile-window deferral.
    fn warm_up(&mut self, nodes: &[NodeId]) {
        for _ in 0..2 {
            self.frame(nodes);
        }
    }

    fn status(&self, node: NodeId) -> NodeRuntimeStatus {
        self.engine
            .tree()
            .get(node)
            .expect("node entry")
            .status
            .value()
            .clone()
    }

    /// Register the `bus:time` product binding P4 will make real.
    fn publish_time_product(&mut self, timebase: NodeId) {
        self.add_literal(
            LpValue::Product(ProductRef::Time(TimeProduct::new(timebase, 0))),
            "time",
            Kind::Instant,
        );
    }

    /// Write a phasor config onto a bus channel — the "driven period" path.
    ///
    /// `Kind::Color` is not a statement about phasors: bus kinds are
    /// first-claim-wins and the loader derives a consumed binding's kind from
    /// its SLOT NAME, so the reader's `wave` binding claims the channel as
    /// Color before this writer ever sees it. (`Kind` is legacy — the
    /// resolver never checks it.)
    fn publish_config(&mut self, channel: &str, config: &PhasorConfig) {
        self.add_literal(config.to_lp_value(), channel, Kind::Color);
    }

    fn add_literal(&mut self, value: LpValue, channel: &str, kind: Kind) {
        self.literals
            .push((value.clone(), String::from(channel), kind));
        self.register_literal(value, channel, kind);
    }

    fn register_literal(&mut self, value: LpValue, channel: &str, kind: Kind) {
        let owner = self.engine.tree().root();
        let revision = self.engine.revision();
        self.engine
            .add_binding(
                BindingDraft {
                    source: BindingSource::Literal(value),
                    target: BindingTarget::BusChannel(ChannelName(String::from(channel))),
                    priority: BindingPriority::authored(),
                    kind,
                    owner,
                },
                revision,
            )
            .expect("register literal binding");
    }

    /// Publish a timebase directly, standing in for a clock whose rate,
    /// scrub and pause the project codec will not round-trip (clock controls
    /// are `Debug`-role slot data; their effect on what a clock publishes is
    /// pinned at the node in `nodes::clock::clock_node`). Everything a
    /// timebase uniform can observe about a clock arrives through exactly
    /// these two numbers.
    fn publish_timebase(&mut self, timebase: NodeId, seconds: f32, delta: f32) {
        let revision = self.engine.revision();
        self.engine
            .timebases_mut()
            .set_timebase(timebase, seconds, delta, revision);
    }

    /// Write one of the clock's transient controls the way the studio's
    /// Debug slider does: a slot edit staged in the registry overlay, then
    /// applied to the engine.
    ///
    /// This is the only way to drive `play_state` / `scrub_offset_seconds` —
    /// they are `Debug`-role slot data, which the project codec deliberately
    /// round-trips to defaults, so authoring them into the fixture's
    /// `clock.json` would be silently discarded (P2/P3 both hit this).
    ///
    /// Only the scrub walkthrough drives this, and that test is host-tier.
    #[cfg(feature = "scrub-log")]
    fn write_clock_control(&mut self, path: &str, value: LpValue) {
        let shapes = self.engine.slot_shapes().clone();
        self.revision += 1;
        let result = self
            .registry
            .mutate(
                &self.fs,
                lpc_model::MutationOp::PutSlotEdit {
                    artifact: lpc_model::ArtifactLocation::file("/clock.json"),
                    edit: lpc_model::SlotEdit::assign_value(
                        SlotPath::parse(path).expect("slot path"),
                        value,
                    ),
                },
                Revision::new(self.revision),
                &ParseCtx { shapes: &shapes },
            )
            .expect("write clock control");
        self.engine
            .apply_project_changes(&self.fs, &mut self.registry, &result.changes)
            .expect("apply project changes");
        for (value, channel, kind) in core::mem::take(&mut self.literals) {
            self.add_literal(value, &channel, kind);
        }
    }

    /// Re-read one artifact and apply it, the way an authoring edit does.
    fn apply_edit(&mut self, path: &str) {
        let shapes = self.engine.slot_shapes().clone();
        self.revision += 1;
        let changes = self.registry.refresh_artifacts(
            &self.fs,
            &[FsEvent {
                path: LpPathBuf::from(path),
                kind: FsEventKind::Modify,
            }],
            Revision::new(self.revision),
            &ParseCtx { shapes: &shapes },
        );
        self.engine
            .apply_project_changes(&self.fs, &mut self.registry, &changes)
            .expect("apply project changes");
        for (value, channel, kind) in core::mem::take(&mut self.literals) {
            self.add_literal(value, &channel, kind);
        }
    }
}

// --- Project fixtures ------------------------------------------------------

fn write(fs: &LpFsMemory, path: &str, body: &str) {
    let path = String::from(path);
    fs.write_file(path.as_str().as_path(), body.as_bytes())
        .expect("write project file");
}

/// A compute node reading `wave` (phasor) and `elapsed` (seconds).
///
/// The clocked variant also reads a plain f32 `seconds` uniform bound to the
/// clock's produced seconds: that is what puts the clock in the demand walk,
/// so it has published a timebase by the time the phasor asks for one. It is
/// named `seconds` on purpose — the loader derives a binding's bus kind from
/// the slot name, and only `time`/`seconds`/`delta_seconds` claim `Instant`,
/// which is what the clock's own publishing binding claims.
fn compute_json(source: &str, bindings: &str, consumed: &str, phasor: &str) -> String {
    format!(
        r#"
{{
  "kind": "ComputeShader",
  "source": {{ "path": "{source}" }},
  "bindings": {bindings},
  "consumed": {{
    {consumed}
    "wave": {{ "kind": "phasor", "value": "f32", "phasor": {phasor} }},
    "elapsed": {{ "kind": "seconds", "value": "f32" }}
  }},
  "produced": {{
    "out_wave": {{ "kind": "value", "value": "f32" }},
    "out_elapsed": {{ "kind": "value", "value": "f32" }}
  }}
}}
"#
    )
}

const CLOCKED_CONSUMED: &str = r#""seconds": { "kind": "value", "value": "f32", "default": 0.0 },"#;
const CLOCKED_BINDINGS: &str = r#"{ "seconds": { "source": "bus:clock_seconds" } }"#;
const CLOCKED_GLSL: &str =
    "void tick() { out_wave = wave + seconds * 0.0; out_elapsed = elapsed; }";
const PLAIN_GLSL: &str = "void tick() { out_wave = wave; out_elapsed = elapsed; }";

fn ramp(period_seconds: f32) -> String {
    format!(
        "{{ \"period_seconds\": {period_seconds}, \"waveform\": \"ramp\", \"phase_offset\": 0.0 }}"
    )
}

fn clocked_compute_json(phasor: &str) -> String {
    compute_json("clocked.glsl", CLOCKED_BINDINGS, CLOCKED_CONSUMED, phasor)
}

fn load(fs: LpFsMemory) -> Project {
    let services = EngineServices::new(TreePath::parse("/shader_timebase.show").expect("root"));
    let loaded = ProjectLoader::load_from_root(&fs, services).expect("load project");
    let (mut engine, registry) = loaded.into_parts();
    engine.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
        lp_shader::ShaderFrontend::LpsGlsl,
    ))));
    Project {
        engine,
        registry,
        fs,
        revision: 1,
        literals: Vec::new(),
    }
}

/// One compute node plus a real clock, wired so the clock is demanded
/// through an ordinary f32 uniform while `bus:time` stays free for the
/// hand-registered product binding.
///
/// The clock's own `product` output is default-bound to `bus:time` (P4), so
/// these fixtures author it onto a dead channel: the tests below are about
/// what the evaluator does with a given product handle, and a second
/// fallback producer on `time` would make that a statement about binding
/// priority instead.
fn clocked_fs(phasor: &str) -> LpFsMemory {
    let fs = LpFsMemory::new();
    write(&fs, "/project.json", "{ \"format\": 7 }\n");
    write(&fs, "/clocked.glsl", CLOCKED_GLSL);
    write(
        &fs,
        "/clock.json",
        r#"{ "kind": "Clock", "bindings": {
             "seconds": { "target": "bus:clock_seconds" },
             "product": { "target": "bus:clock_product" } } }"#,
    );
    write(&fs, "/compute.json", &clocked_compute_json(phasor));
    write(
        &fs,
        "/module.json",
        r#"
{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "compute": { "ref": "./compute.json" }
  }
}
"#,
    );
    fs
}

/// Two compute nodes with different local shaping, optionally both binding
/// `wave` to one config channel. Whether they share a phase is then a
/// statement about provenance, not about their authoring happening to agree.
fn paired_fs(a_phasor: &str, b_phasor: &str, bind_config: bool) -> LpFsMemory {
    let fs = LpFsMemory::new();
    write(&fs, "/project.json", "{ \"format\": 7 }\n");
    write(&fs, "/plain.glsl", PLAIN_GLSL);
    let bindings = if bind_config {
        r#"{ "wave": { "source": "bus:wave_config" } }"#
    } else {
        "{}"
    };
    write(
        &fs,
        "/a.json",
        &compute_json("plain.glsl", bindings, "", a_phasor),
    );
    write(
        &fs,
        "/b.json",
        &compute_json("plain.glsl", bindings, "", b_phasor),
    );
    write(
        &fs,
        "/module.json",
        r#"
{
  "kind": "Module",
  "nodes": {
    "a": { "ref": "./a.json" },
    "b": { "ref": "./b.json" }
  }
}
"#,
    );
    fs
}

// --- Tests -----------------------------------------------------------------

/// The end-to-end shape: a real clock publishing a real timebase, a
/// hand-registered product handle on `bus:time`, and a phasor uniform that
/// walks its cycle at the authored period.
#[test]
fn a_phasor_uniform_walks_a_real_clocks_timebase() {
    let mut project = load(clocked_fs(&ramp(1.0)));
    let clock = project.node("clock.clock");
    let compute = project.node("compute.compute_shader");
    project.publish_time_product(clock);
    project.warm_up(&[compute]);

    let start = project.read(compute, "out_wave");
    for _ in 0..4 {
        project.frame(&[compute]);
    }
    let end = project.read(compute, "out_wave");

    assert!(
        project.engine.timebases().seconds(clock).is_some(),
        "the clock must be demanded, or there is no timebase to read"
    );
    assert!(
        (end - start - 4.0 * TICK_SECONDS).abs() < 1e-4,
        "a 1 s ramp advances one tick of phase per tick: {start} -> {end}"
    );
    assert_eq!(
        project.status(compute),
        NodeRuntimeStatus::Ok,
        "a resolvable timebase is not a warning"
    );
}

/// Every read inside one frame sees the same phase: the phasor advances once
/// per store tick, not once per consumer.
#[test]
fn repeated_reads_in_one_frame_see_one_advance() {
    let mut project = load(clocked_fs(&ramp(1.0)));
    let clock = project.node("clock.clock");
    let compute = project.node("compute.compute_shader");
    project.publish_time_product(clock);
    project.warm_up(&[compute]);
    project.frame(&[compute]);

    let first = project.read(compute, "out_wave");

    assert_eq!(project.read(compute, "out_wave"), first);
    assert_eq!(project.read(compute, "out_wave"), first);
}

/// The `seconds` kind is the unbounded read: whatever a real clock says
/// "now" is, tick for tick, with no wrapping.
#[test]
fn a_seconds_uniform_tracks_a_real_clocks_published_seconds() {
    let mut project = load(clocked_fs(&ramp(1.0)));
    let clock = project.node("clock.clock");
    let compute = project.node("compute.compute_shader");
    project.publish_time_product(clock);
    project.warm_up(&[compute]);

    for step in 0..4 {
        project.frame(&[compute]);
        let published = project
            .engine
            .timebases()
            .seconds(clock)
            .expect("published");
        let uniform = project.read(compute, "out_elapsed");
        assert!(
            (uniform - published).abs() < 1e-5,
            "frame {step}: seconds uniform {uniform} vs published {published}"
        );
        assert!(published > 0.0, "the clock must actually be running");
    }
}

/// Rate scaling, a scrub jump and a pause reach the uniform, because every
/// one of them reaches the timebase the uniform reads.
#[test]
fn a_seconds_uniform_follows_a_scrub_and_freezes_with_a_paused_timebase() {
    let mut project = load(paired_fs(&ramp(1.0), &ramp(1.0), false));
    let compute = project.node("a.compute_shader");
    let root = project.engine.tree().root();
    project.publish_time_product(root);
    project.publish_timebase(root, 0.0, 0.0);
    project.warm_up(&[compute]);

    // Double rate: twice the seconds per tick, no wrapping in sight.
    for step in 1..=3 {
        let seconds = step as f32 * 2.0 * TICK_SECONDS;
        project.publish_timebase(root, seconds, 2.0 * TICK_SECONDS);
        project.frame(&[compute]);
        let uniform = project.read(compute, "out_elapsed");
        assert!(
            (uniform - seconds).abs() < 1e-5,
            "step {step}: {uniform} vs {seconds}"
        );
    }

    // Scrubbed far forward: an unbounded read follows the jump.
    project.publish_timebase(root, 1_234.5, 0.0);
    project.frame(&[compute]);
    assert!(
        (project.read(compute, "out_elapsed") - 1_234.5).abs() < 1e-2,
        "the uniform follows the timebase wherever it goes: {}",
        project.read(compute, "out_elapsed")
    );

    // Paused: seconds hold, and so does every phasor riding on them.
    let held_phase = project.read(compute, "out_wave");
    for _ in 0..3 {
        project.publish_timebase(root, 1_234.5, 0.0);
        project.frame(&[compute]);
        assert!(
            (project.read(compute, "out_elapsed") - 1_234.5).abs() < 1e-2,
            "a paused timebase freezes the seconds uniform"
        );
        assert!(
            (project.read(compute, "out_wave") - held_phase).abs() < 1e-6,
            "…and every phasor riding on it"
        );
    }
}

/// Re-authoring the period must change the RATE from that instant, never the
/// phase: the store keeps no config, and the evaluator hands it a fresh one
/// every tick. A period drag that jumped the phase would be visible as a
/// flash on every LED in the room.
#[test]
fn a_live_period_edit_changes_the_rate_without_disturbing_the_phase() {
    let mut project = load(clocked_fs(&ramp(1.0)));
    let clock = project.node("clock.clock");
    let compute = project.node("compute.compute_shader");
    project.publish_time_product(clock);
    project.warm_up(&[compute]);
    for _ in 0..3 {
        project.frame(&[compute]);
    }
    let before = project.read(compute, "out_wave");
    assert!(before > 0.0, "phase before the edit: {before}");

    write(
        &project.fs,
        "/compute.json",
        &clocked_compute_json(&ramp(2.0)),
    );
    project.apply_edit("/compute.json");
    project.frame(&[compute]);
    let at_edit = project.read(compute, "out_wave");

    // The phase itself never moves — that is the whole subject. Where the
    // frame carrying the edit spends its delta differs by tier: with the
    // breakpoint log the edit lands at the effective time it arrived, so
    // that frame finishes at the OLD rate; the forward-only integrator
    // spends the whole frame at the new one.
    assert!(
        at_edit > before && at_edit - before <= TICK_SECONDS + 1e-4,
        "the edit must not displace the phase: {before} -> {at_edit}"
    );
    #[cfg(feature = "scrub-log")]
    assert!(
        (at_edit - before - TICK_SECONDS).abs() < 1e-4,
        "the frame across the edit finishes at the OLD rate: {before} -> {at_edit}"
    );
    #[cfg(not(feature = "scrub-log"))]
    assert!(
        (at_edit - before - 0.5 * TICK_SECONDS).abs() < 1e-4,
        "the frame across the edit advances at the NEW rate from the OLD \
         phase: {before} -> {at_edit}"
    );
    project.frame(&[compute]);
    assert!(
        (project.read(compute, "out_wave") - at_edit - 0.5 * TICK_SECONDS).abs() < 1e-4,
        "and keeps running at the new rate"
    );
}

/// Period 0 means frozen, not reset — the phasor holds whatever it had.
#[test]
fn a_zero_period_freezes_the_uniform_where_it_stands() {
    let mut project = load(clocked_fs(&ramp(1.0)));
    let clock = project.node("clock.clock");
    let compute = project.node("compute.compute_shader");
    project.publish_time_product(clock);
    project.warm_up(&[compute]);
    for _ in 0..3 {
        project.frame(&[compute]);
    }
    let before = project.read(compute, "out_wave");
    assert!(before > 0.0);

    write(
        &project.fs,
        "/compute.json",
        &clocked_compute_json(&ramp(0.0)),
    );
    project.apply_edit("/compute.json");
    // The freeze takes hold at the effective time it arrived — with the
    // breakpoint log that is *now*, so the frame carrying the edit finishes
    // at the old rate; the forward-only integrator stops it a frame earlier.
    // Either way the phase is held, never reset.
    project.frame(&[compute]);
    let held = project.read(compute, "out_wave");
    assert!(
        held >= before && held - before <= TICK_SECONDS + 1e-4,
        "the freeze must hold the phase, not move it: {before} -> {held}"
    );

    // …and from there nothing moves it. Frozen, not reset: a phasor that
    // snapped back to zero here would flash every LED in the room.
    for _ in 0..3 {
        project.frame(&[compute]);
        assert!(
            (project.read(compute, "out_wave") - held).abs() < 1e-6,
            "a frozen phasor holds its phase rather than resetting it"
        );
    }
    assert!(held > 0.0, "…and what it holds is where it had got to");
}

/// D3, the shared half: a config channel with a writer means ONE integrator
/// for every reader of that channel, whatever each slot authored locally.
#[test]
fn two_nodes_driven_by_one_config_channel_share_a_phase() {
    let mut project = load(paired_fs(&ramp(1.0), &ramp(8.0), true));
    let a = project.node("a.compute_shader");
    let b = project.node("b.compute_shader");
    let root = project.engine.tree().root();
    project.publish_time_product(root);
    project.publish_config("wave_config", &PhasorConfig::with_period(2.0));
    project.publish_timebase(root, 0.0, 0.0);
    project.warm_up(&[a, b]);

    for step in 1..=5 {
        project.publish_timebase(root, step as f32 * TICK_SECONDS, TICK_SECONDS);
        project.frame(&[a, b]);
        let (wave_a, wave_b) = (project.read(a, "out_wave"), project.read(b, "out_wave"));
        assert!(
            (wave_a - wave_b).abs() < 1e-6,
            "step {step}: one channel, one integrator: {wave_a} vs {wave_b}"
        );
    }

    // …and it is the CHANNEL's period that is running, not either local one.
    let expected = 5.0 * TICK_SECONDS / 2.0;
    let wave_a = project.read(a, "out_wave");
    assert!(
        (wave_a - expected).abs() < 1e-4,
        "the driven period wins: {wave_a} vs {expected}"
    );
    assert_eq!(
        project
            .engine
            .timebases()
            .entry(root)
            .expect("timebase")
            .phasor_count(),
        1,
        "one shared integrator for both readers"
    );
}

/// D3, the private half: binding a config channel nobody writes is an R6
/// fallback to the authored config, and an R6 fallback is exactly as
/// slot-local as no binding at all. Two nodes must NOT collapse onto one
/// integrator just because they name the same empty channel.
#[test]
fn an_unwritten_config_channel_leaves_each_node_private() {
    let mut project = load(paired_fs(&ramp(1.0), &ramp(8.0), true));
    let a = project.node("a.compute_shader");
    let b = project.node("b.compute_shader");
    let root = project.engine.tree().root();
    project.publish_time_product(root);
    project.publish_timebase(root, 0.0, 0.0);
    project.warm_up(&[a, b]);

    for step in 1..=4 {
        project.publish_timebase(root, step as f32 * TICK_SECONDS, TICK_SECONDS);
        project.frame(&[a, b]);
    }

    let (wave_a, wave_b) = (project.read(a, "out_wave"), project.read(b, "out_wave"));
    assert!(
        (wave_a - 4.0 * TICK_SECONDS).abs() < 1e-4,
        "a runs its own 1 s period: {wave_a}"
    );
    assert!(
        (wave_b - 4.0 * TICK_SECONDS / 8.0).abs() < 1e-4,
        "b runs its own 8 s period: {wave_b}"
    );
    assert_eq!(
        project
            .engine
            .timebases()
            .entry(root)
            .expect("timebase")
            .phasor_count(),
        2,
        "two private integrators, not one shared one"
    );
}

/// "Grabbing the reins": when a channel starts driving a config, identity
/// moves from `Private` to `Shared`, and a different integrator means the
/// phase restarts. That reset is the intended, visible signal that something
/// else now owns this phasor.
#[test]
fn a_channel_taking_over_a_local_config_resets_the_phase() {
    let mut project = load(paired_fs(&ramp(1.0), &ramp(1.0), true));
    let a = project.node("a.compute_shader");
    let root = project.engine.tree().root();
    project.publish_time_product(root);
    project.publish_timebase(root, 0.0, 0.0);
    project.warm_up(&[a]);

    for step in 1..=4 {
        project.publish_timebase(root, step as f32 * TICK_SECONDS, TICK_SECONDS);
        project.frame(&[a]);
    }
    let before = project.read(a, "out_wave");
    assert!(
        (before - 4.0 * TICK_SECONDS).abs() < 1e-4,
        "private phase: {before}"
    );

    project.publish_config("wave_config", &PhasorConfig::with_period(1.0));
    project.publish_timebase(root, 0.5, TICK_SECONDS);
    project.frame(&[a]);

    let after = project.read(a, "out_wave");
    assert!(
        (after - TICK_SECONDS).abs() < 1e-4,
        "the shared integrator is newly materialized, so the uniform \
         restarts from the top of its cycle: {before} -> {after}"
    );
}

/// The waveform is slot-local even when the period is not: two readers of
/// one config channel see the same cycle through their own shaping, and the
/// channel's own waveform field is nobody's business.
#[test]
fn a_shared_config_still_leaves_the_waveform_slot_local() {
    let square = "{ \"period_seconds\": 1.0, \"waveform\": \"square\", \"phase_offset\": 0.0 }";
    let mut project = load(paired_fs(&ramp(1.0), square, true));
    let a = project.node("a.compute_shader");
    let b = project.node("b.compute_shader");
    let root = project.engine.tree().root();
    project.publish_time_product(root);
    project.publish_config(
        "wave_config",
        &PhasorConfig {
            period_seconds: 1.0,
            waveform: Waveform::Sine,
            phase_offset: 0.0,
        },
    );
    project.publish_timebase(root, 0.0, 0.0);
    project.warm_up(&[a, b]);

    for step in 1..=7 {
        project.publish_timebase(root, step as f32 * TICK_SECONDS, TICK_SECONDS);
        project.frame(&[a, b]);
    }

    let ramp_reader = project.read(a, "out_wave");
    assert!(
        (ramp_reader - 0.7).abs() < 1e-4,
        "the ramp reader sees the raw cycle: {ramp_reader}"
    );
    assert_eq!(
        project.read(b, "out_wave"),
        1.0,
        "the square reader sees the same cycle, squared"
    );

    // Clock-face-v2 P1: the shared integrator's probe row lists BOTH
    // readers with their own shaping — one integrator, two readings.
    let result = project
        .engine
        .read_project_timebase_probe(lpc_wire::TimebaseProbeRequest {
            product: TimeProduct::new(root, 0),
        });
    let lpc_wire::TimebaseProbeResult::Timebase { phasors, .. } = result else {
        panic!("a producing clock resolves a timebase, got {result:?}");
    };
    assert_eq!(phasors.len(), 1, "one shared integrator: {phasors:?}");
    let mut readings = phasors[0].readings.clone();
    readings.sort_by_key(|reading| reading.node);
    assert_eq!(
        readings
            .iter()
            .map(|reading| (reading.node, reading.slot.as_str(), reading.waveform))
            .collect::<Vec<_>>(),
        vec![
            (a.0, "wave", Waveform::Ramp),
            (b.0, "wave", Waveform::Square),
        ],
        "two shaped readings of one shared cycle"
    );
}

/// P7 item 4: the timebase probe is the ONLY way a client can see what is
/// riding a clock, and it reports the store as it stands — a private
/// integrator named by node+slot, a shared one named by its channel, each
/// with the period it last ran at.
#[test]
fn the_timebase_probe_lists_what_rides_the_clock() {
    let mut project = load(paired_fs(&ramp(1.0), &ramp(8.0), false));
    let a = project.node("a.compute_shader");
    let b = project.node("b.compute_shader");
    let root = project.engine.tree().root();
    project.publish_time_product(root);
    project.publish_timebase(root, 0.0, 0.0);
    project.warm_up(&[a, b]);
    project.publish_timebase(root, TICK_SECONDS, TICK_SECONDS);
    project.frame(&[a, b]);

    let result = project
        .engine
        .read_project_timebase_probe(lpc_wire::TimebaseProbeRequest {
            product: TimeProduct::new(root, 0),
        });
    let lpc_wire::TimebaseProbeResult::Timebase {
        seconds, phasors, ..
    } = result
    else {
        panic!("a producing clock resolves a timebase, got {result:?}");
    };
    assert!((seconds - TICK_SECONDS).abs() < 1e-6);
    assert_eq!(phasors.len(), 2, "two private integrators: {phasors:?}");
    let periods: Vec<f32> = phasors.iter().map(|row| row.period_seconds).collect();
    assert!(periods.contains(&1.0) && periods.contains(&8.0));
    for row in &phasors {
        let lpc_wire::WirePhasorOrigin::Node { node, slot } = &row.origin else {
            panic!("an unwired config is private to its slot, got {row:?}");
        };
        assert_eq!(slot, "wave", "the CONSUMED slot, not the config field");
        assert!((0.0..1.0).contains(&row.phase), "raw ramp: {row:?}");
        // Clock-face-v2 P1: the row carries its readings — a private
        // integrator has exactly its own consumer, shaping and all.
        assert_eq!(
            row.readings,
            vec![lpc_wire::WirePhasorReading {
                node: *node,
                slot: String::from("wave"),
                waveform: Waveform::Ramp,
                phase_offset: 0.0,
            }],
            "one reader on a private integrator: {row:?}"
        );
    }

    // A product naming a node that publishes no timebase is a structured
    // answer, not an error: a card asking about a node that just left the
    // tree is not a fault.
    assert!(matches!(
        project
            .engine
            .read_project_timebase_probe(lpc_wire::TimebaseProbeRequest {
                product: TimeProduct::new(a, 0),
            }),
        lpc_wire::TimebaseProbeResult::Unknown { .. }
    ));
}

/// No writer for `bus:time` anywhere is the normal state of the world until
/// P4: the timebase kinds must fall back to their shaped defaults and warn,
/// never leave the backend a uniform short.
#[test]
fn timebase_uniforms_without_a_product_run_at_their_shaped_default_and_warn() {
    let triangle =
        "{ \"period_seconds\": 4.0, \"waveform\": \"triangle\", \"phase_offset\": 0.25 }";
    let mut project = load(clocked_fs(triangle));
    let compute = project.node("compute.compute_shader");
    // Deliberately no `publish_time_product`.
    project.warm_up(&[compute]);
    project.frame(&[compute]);

    let wave = project.read(compute, "out_wave");
    assert!(
        (wave - 0.5).abs() < 1e-6,
        "a triangle at its 0.25 offset: {wave}"
    );
    assert_eq!(project.read(compute, "out_elapsed"), 0.0);
    let NodeRuntimeStatus::Warn(message) = project.status(compute) else {
        panic!("expected a Warn, got {:?}", project.status(compute));
    };
    assert!(
        message.contains("input \"wave\"") && message.contains("input \"elapsed\""),
        "both timebase uniforms report themselves: {message}"
    );
}

/// D12's loud failure: a plain `f32` uniform bound to a channel that carries
/// a Product cannot convert, so it runs on its authored default AND says so.
/// This is what stops the post-P4 world from silently freezing every
/// un-migrated `time` uniform.
#[test]
fn an_f32_uniform_on_a_product_channel_warns_instead_of_freezing_silently() {
    let fs = LpFsMemory::new();
    write(&fs, "/project.json", "{ \"format\": 7 }\n");
    write(&fs, "/plain.glsl", "void tick() { out_time = time; }");
    write(
        &fs,
        "/compute.json",
        r#"
{
  "kind": "ComputeShader",
  "source": { "path": "plain.glsl" },
  "bindings": { "time": { "source": "bus:time" } },
  "consumed": { "time": { "kind": "value", "value": "f32", "default": 7.5 } },
  "produced": { "out_time": { "kind": "value", "value": "f32" } }
}
"#,
    );
    write(
        &fs,
        "/module.json",
        r#"{ "kind": "Module", "nodes": { "compute": { "ref": "./compute.json" } } }"#,
    );
    let mut project = load(fs);
    let compute = project.node("compute.compute_shader");
    let root = project.engine.tree().root();
    project.publish_time_product(root);

    for _ in 0..3 {
        project
            .engine
            .tick(&project.registry, TICK_MS)
            .expect("tick");
        project.read(compute, "out_time");
    }

    assert!(
        (project.read(compute, "out_time") - 7.5).abs() < 1e-6,
        "the shader keeps running on its authored default"
    );
    let NodeRuntimeStatus::Warn(message) = project.status(compute) else {
        panic!("expected a Warn, got {:?}", project.status(compute));
    };
    assert!(
        message.contains("input \"time\" using its default"),
        "the warning must name the input: {message}"
    );
}

/// The caveat PR #316's doc comment left open, now settled the right way
/// round: an UNBOUND shader uniform resolves `Ok` through the
/// authored-default projection
/// (`EngineResolveHost::read_shader_consumed_slot_default`, the
/// 2026-08-04-unbound-shader-uniform-warns fix) and the node reports `Ok`.
/// This test pinned the *defective* Warn until that fix merged; it is now
/// the positive pin the old assertion message promised.
///
/// The kind-mismatch test above stays built on an *authored binding* — that
/// warning is earned, and must remain distinguishable from a slot that is
/// simply unbound.
#[test]
fn an_unbound_uniform_runs_quietly_on_its_authored_default() {
    let fs = LpFsMemory::new();
    write(&fs, "/project.json", "{ \"format\": 7 }\n");
    write(&fs, "/plain.glsl", "void tick() { out_t = t; }");
    write(
        &fs,
        "/compute.json",
        r#"
{
  "kind": "ComputeShader",
  "source": { "path": "plain.glsl" },
  "consumed": { "t": { "kind": "value", "value": "f32", "default": 3.25 } },
  "produced": { "out_t": { "kind": "value", "value": "f32" } }
}
"#,
    );
    write(
        &fs,
        "/module.json",
        r#"{ "kind": "Module", "nodes": { "compute": { "ref": "./compute.json" } } }"#,
    );
    let mut project = load(fs);
    let compute = project.node("compute.compute_shader");

    for _ in 0..3 {
        project
            .engine
            .tick(&project.registry, TICK_MS)
            .expect("tick");
        project.read(compute, "out_t");
    }

    assert!(
        (project.read(compute, "out_t") - 3.25).abs() < 1e-6,
        "the authored default is what the uniform runs on"
    );
    assert!(
        matches!(project.status(compute), NodeRuntimeStatus::Ok),
        "an unbound uniform behaving exactly as authored must not warn: {:?}",
        project.status(compute)
    );
}

// --- P8: scrubbing the Debug slider ----------------------------------------

/// The sim walkthrough: drive `scrub_offset_seconds` exactly as the studio's
/// Debug slider does and watch a phasor uniform come back **bit for bit**.
///
/// The clock is paused first, so wall time is not moving underneath the
/// slider and the offset alone says where the clock is — which is what a
/// transport scrub means. Every step here is the real path: an overlay slot
/// edit, `apply_project_changes`, the clock node re-resolving its controls,
/// the timebase it publishes, the store's breakpoint log, and the uniform
/// fill that shapes the ramp.
///
/// Host-tier only: a firmware build has no breakpoint log to reconstruct
/// from, and no transport UI to ask it to. What a device does with a
/// backward scrub is pinned at the clock instead
/// (`a_backward_scrub_publishes_a_negative_delta`).
#[cfg(feature = "scrub-log")]
#[test]
fn scrubbing_the_debug_slider_reproduces_a_phasor_uniform_exactly() {
    let mut project = load(clocked_fs(&ramp(1.0)));
    let clock = project.node("clock.clock");
    let compute = project.node("compute.compute_shader");
    project.publish_time_product(clock);
    project.warm_up(&[compute]);
    for _ in 0..6 {
        project.frame(&[compute]);
    }
    let live_edge = project
        .engine
        .timebases()
        .seconds(clock)
        .expect("published");
    assert!(live_edge > 0.0, "the clock has to have run: {live_edge}");

    project.write_clock_control(
        "transport.play_state",
        LpValue::String(lpc_model::PlayState::Paused.as_str().to_string()),
    );
    project.frame(&[compute]);
    let paused_at = project
        .engine
        .timebases()
        .seconds(clock)
        .expect("published");

    // Three slider positions behind the live edge, remembered.
    let offsets = [-0.45, -0.25, -0.1];
    let mut seen = Vec::new();
    for offset in offsets {
        project.write_clock_control("transport.scrub_offset_seconds", LpValue::F32(offset));
        project.frame(&[compute]);
        let entry = project.engine.timebases().entry(clock).expect("timebase");
        assert!(
            entry
                .live_edge()
                .is_some_and(|edge| entry.effective_seconds < edge),
            "the slider must put the clock BEHIND the live edge — otherwise \
             this reads the forward path and proves nothing: {} vs {:?}",
            entry.effective_seconds,
            entry.live_edge()
        );
        seen.push(project.read(compute, "out_wave"));
    }
    assert!(
        seen.windows(2).all(|pair| pair[0] != pair[1]),
        "three slider positions must show three different frames: {seen:?}"
    );

    // Revisited in the other order, they read the same frames.
    for (offset, expected) in offsets.iter().zip(seen).rev() {
        project.write_clock_control("transport.scrub_offset_seconds", LpValue::F32(*offset));
        project.frame(&[compute]);
        assert_eq!(
            project.read(compute, "out_wave"),
            expected,
            "the uniform at slider position {offset} did not reproduce"
        );
    }

    // Releasing the slider puts the clock back at the live edge, and the
    // phasor picks up from what it was showing there.
    project.write_clock_control("transport.scrub_offset_seconds", LpValue::F32(0.0));
    project.frame(&[compute]);
    assert!(
        (project
            .engine
            .timebases()
            .seconds(clock)
            .expect("published")
            - paused_at)
            .abs()
            < 1e-6,
        "back to the live edge"
    );
    assert_eq!(
        project.status(compute),
        NodeRuntimeStatus::Ok,
        "scrubbing is not an error condition"
    );
}
