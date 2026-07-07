//! End-to-end edit flow against an in-process LightPlayer server.
//!
//! Harness-level, no UI: a real `LpServer` (simulator session) runs behind a
//! `ClientIo` adapter that pumps every client message through
//! `LpServer::tick_and_send`. The studio actor drives the same command path
//! the web shell uses: connect → `SetValue` on a clock control (transient)
//! and a fixture slot (persisted) → observe DTO dirty states → `SaveOverlay`
//! (def file on disk gains only the persisted edit) → `RevertAllEdits`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::{Pin, pin};
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use lpa_client::ClientIo;
use lpa_server::{Graphics, LpGraphics, LpServer};
use lpc_model::{AsLpPath, LpValue, SlotPath};
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::ServerTransport;
use lpc_wire::{
    ClientMessage, ClientRequest, TransportError, WireMessage, WireProjectCommand,
    WireServerMessage,
};
use lpfs::LpFsMemory;

use crate::{
    ControllerId, ProjectController, ProjectOp, SlotEditOp, StudioActor, StudioCommand,
    StudioController, StudioServerClient, UiAction, UiConfigSlot, UiConfigSlotBody,
    UiNodeDirtyState, UiNodeSection, UiNodeTabBody, UiStudioView, UiViewContent,
};

#[test]
fn simulator_session_edit_save_and_revert_end_to_end() {
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::clone(&sent),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    // Connect the running project through the real client path so the
    // inventory read installs the node → def-artifact map.
    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");

    let rate = find_slot(&snapshot, "controls.rate");
    assert_eq!(rate.state.dirty, UiNodeDirtyState::Clean);
    assert!(rate.state.live, "clock rate is a transient (live) control");
    let rate_address = rate.address.clone().expect("rate slot carries an address");
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(color_order.state.dirty, UiNodeDirtyState::Clean);
    assert!(!color_order.state.live, "color order is a persisted slot");
    let color_order_address = color_order
        .address
        .clone()
        .expect("color order slot carries an address");
    assert_eq!(editor_dirty(&snapshot), (0, 0));

    // An oninput flood on the clock rate plus one persisted edit, queued into
    // one actor batch: the flood coalesces to a single mutation per address.
    let mutations_before = count_mutations(&sent);
    for value in [1.2_f32, 1.6, 2.0] {
        handle
            .tx
            .send(set_value_action(rate_address.clone(), LpValue::F32(value)));
    }
    handle.tx.send(set_value_action(
        color_order_address.clone(),
        LpValue::String("rgb".to_string()),
    ));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("edits emit a snapshot");

    assert_eq!(
        count_mutations(&sent) - mutations_before,
        2,
        "three queued rate SetValues coalesce with the color-order edit into two mutations"
    );
    let rate = find_slot(&snapshot, "controls.rate");
    assert_eq!(rate.state.dirty, UiNodeDirtyState::Dirty);
    assert!(rate.state.live);
    assert_eq!(slot_value_display(rate), "2");
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(color_order.state.dirty, UiNodeDirtyState::Dirty);
    assert!(!color_order.state.live);
    assert_eq!(slot_value_display(color_order), "rgb");
    assert_eq!(
        editor_dirty(&snapshot),
        (1, 1),
        "one persisted and one transient slot are dirty"
    );

    // Save: the persisted color-order edit commits to fixture.json; the
    // transient rate edit stays pending (dirty-live), clock.json untouched.
    handle.tx.send(project_action(ProjectOp::SaveOverlay));
    drive(actor.run_one_batch_for_test());
    // Pull a refresh so the synced view reflects the committed def.
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("save + refresh emit a snapshot");

    let fixture_json = read_project_file(&server, "fixture.json");
    assert!(
        fixture_json.contains("\"color_order\":\"rgb\""),
        "fixture.json gained the persisted color-order edit: {fixture_json}"
    );
    let clock_json = read_project_file(&server, "clock.json");
    assert!(
        !clock_json.contains("\"rate\":2"),
        "clock.json must not gain the transient rate edit: {clock_json}"
    );
    let rate = find_slot(&snapshot, "controls.rate");
    assert_eq!(
        rate.state.dirty,
        UiNodeDirtyState::Dirty,
        "transient edit survives the save as dirty-live"
    );
    assert_eq!(slot_value_display(rate), "2");
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(color_order.state.dirty, UiNodeDirtyState::Clean);
    assert_eq!(
        slot_value_display(color_order),
        "rgb",
        "committed value synced back"
    );
    assert_eq!(editor_dirty(&snapshot), (0, 1));

    // Revert all: the overlay clears, every slot returns to Clean, and the
    // *gated* refresh (since = last known revision) delivers the reverted
    // def values directly — no reconnect/full resync. Reverting advances the
    // effective def revisions monotonically (studio editing ADR follow-up
    // (e)), so the delta read includes the reverted roots.
    handle.tx.send(project_action(ProjectOp::RevertAllEdits));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("revert emits a snapshot");

    let rate = find_slot(&snapshot, "controls.rate");
    assert_eq!(rate.state.dirty, UiNodeDirtyState::Clean);
    assert_eq!(
        slot_value_display(rate),
        "1",
        "rate reverted to the authored value through the gated refresh"
    );
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(color_order.state.dirty, UiNodeDirtyState::Clean);
    assert_eq!(
        slot_value_display(color_order),
        "rgb",
        "revert does not undo committed file changes"
    );
    assert_eq!(editor_dirty(&snapshot), (0, 0));
}

#[test]
fn per_slot_transient_reset_reverts_value_through_gated_refresh() {
    // The per-slot Reset affordance on a transient control (the clock `rate`
    // slider): SetValue then `SlotEditOp::Revert` must bring the DTO back to
    // the authored default through a *gated* refresh, without a reconnect.
    // The intermediate refresh below syncs the mutated def into the view
    // first, so the final assertion can only pass if the refresh after the
    // revert delivers the *reverted* def root (monotonic revisions, studio
    // editing ADR follow-up (e)) — not because a stale mirror or buffer
    // entry happened to shadow the right value.
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    let rate = find_slot(&snapshot, "controls.rate");
    assert_eq!(slot_value_display(rate), "1");
    let rate_address = rate.address.clone().expect("rate slot carries an address");

    // Edit the transient control, then pull a gated refresh so the synced
    // view itself holds the edited value.
    handle
        .tx
        .send(set_value_action(rate_address.clone(), LpValue::F32(2.0)));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("edit + refresh emit a snapshot");
    let rate = find_slot(&snapshot, "controls.rate");
    assert_eq!(rate.state.dirty, UiNodeDirtyState::Dirty);
    assert_eq!(slot_value_display(rate), "2");

    // Per-slot reset: revert the rate edit, then a gated refresh must show
    // the authored default again.
    handle.tx.send(revert_action(rate_address));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("revert + refresh emit a snapshot");

    let rate = find_slot(&snapshot, "controls.rate");
    assert_eq!(rate.state.dirty, UiNodeDirtyState::Clean);
    assert_eq!(
        slot_value_display(rate),
        "1",
        "per-slot reset restores the authored default through the gated refresh"
    );
}

#[test]
fn set_back_to_base_normalizes_to_clean_without_overlay_fetch() {
    // Minimal-diff normalization, user scenario: pick a choice value
    // (diagnostic-mode style), use it, set it back to the authored value —
    // the edited highlight must clear. The server elides the base-equal
    // assignment (NormalizedToRemoval) and the mirror must learn that from
    // the ack alone: the overlay revision may not advance, so a corrective
    // ReadOverlay would never fire.
    //
    // The refresh between the two edits is load-bearing: it syncs the edited
    // value into the project view, so the set-back ack opens the stale-view
    // window (the view still holds the old effective value until the next
    // gated read). The DTO must keep showing the value the user typed through
    // that window — the buffer entry parks as `AwaitingRefresh` instead of
    // releasing — not jitter back to the superseded value.
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::clone(&sent),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(color_order.state.dirty, UiNodeDirtyState::Clean);
    assert_eq!(slot_value_display(color_order), "grb", "authored default");
    let address = color_order
        .address
        .clone()
        .expect("color order slot carries an address");

    // Change the choice: dirty, counted; the refresh syncs the edited value
    // into the project view (the stale-window precondition).
    handle.tx.send(set_value_action(
        address.clone(),
        LpValue::String("rgb".to_string()),
    ));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("edit + refresh emit a snapshot");
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(color_order.state.dirty, UiNodeDirtyState::Dirty);
    assert_eq!(
        slot_value_display(color_order),
        "rgb",
        "the synced view holds the edited effective value"
    );
    assert_eq!(editor_dirty(&snapshot), (1, 0));

    // Set it back to the authored value. The ack normalizes the edit away,
    // but the synced view still holds "rgb" until the next gated read: the
    // DTO must keep showing the typed value ("grb"), not jitter back.
    let overlay_reads_before = count_overlay_reads(&sent);
    handle.tx.send(set_value_action(
        address,
        LpValue::String("grb".to_string()),
    ));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("set-back emits a snapshot");
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(
        slot_value_display(color_order),
        "grb",
        "the typed base value stays visible through the stale-view window"
    );
    assert_eq!(
        color_order.state.dirty,
        UiNodeDirtyState::Saving,
        "the normalized edit keeps the Saving treatment until the view catches up"
    );

    // The next refresh delivers the reverted def: highlight cleared, value
    // stable, and no overlay fetch corrected the mirror — the ack effect
    // alone did it.
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("refresh emits a snapshot");
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(
        color_order.state.dirty,
        UiNodeDirtyState::Clean,
        "setting a slot back to its base value clears the edited state"
    );
    assert_eq!(
        slot_value_display(color_order),
        "grb",
        "the value never rubber-bands through the whole set-back"
    );
    assert_eq!(editor_dirty(&snapshot), (0, 0));
    assert_eq!(
        count_overlay_reads(&sent) - overlay_reads_before,
        0,
        "the mirror is corrected by the ack effect, not a ReadOverlay"
    );
}

#[test]
fn composite_gesture_cycle_ends_clean_end_to_end() {
    // The M3 composite gesture cycle on the fixture `mapping` slot, driven
    // through the same actor command path the web shell uses: switch the
    // enum variant (EnsurePresent mapping.PathPoints), add a map entry
    // (EnsurePresent mapping.PathPoints.paths[0]), remove it again
    // (RemoveValue — the server normalizes the add-then-remove away, D2),
    // then revert the variant switch — the project must end clean.
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    let mapping = find_slot(&snapshot, "mapping");
    assert_eq!(mapping.state.dirty, UiNodeDirtyState::Clean);
    assert_eq!(mapping.detail.as_deref(), Some("variant Unset"));
    let mapping_address = mapping
        .address
        .clone()
        .expect("mapping slot carries an address");
    assert_eq!(editor_dirty(&snapshot), (0, 0));

    // Switch the variant. The overlay edit is stored at a path with no row
    // yet (the base variant is still Unset until the refresh applies), so
    // the enum row reads dirty through the prefix join immediately.
    let variant_address = child_address(&mapping_address, "mapping.PathPoints");
    handle
        .tx
        .send(ensure_present_action(variant_address.clone()));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("variant switch emits a snapshot");
    let mapping = find_slot(&snapshot, "mapping");
    assert_eq!(
        mapping.state.dirty,
        UiNodeDirtyState::Dirty,
        "the acked variant switch surfaces on the enum row before any refresh"
    );
    assert_eq!(mapping.detail.as_deref(), Some("variant Unset"));
    assert_eq!(
        mapping.edit_entry_address,
        Some(variant_address.clone()),
        "the enum row offers the variant-switch entry as its revert target \
         even before the view's active variant catches up"
    );
    assert_eq!(editor_dirty(&snapshot), (1, 0));

    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("refresh emits a snapshot");
    let mapping = find_slot(&snapshot, "mapping");
    assert_eq!(mapping.detail.as_deref(), Some("variant PathPoints"));
    assert_eq!(mapping.state.dirty, UiNodeDirtyState::Dirty);
    assert_eq!(
        mapping.edit_entry_address,
        Some(variant_address.clone()),
        "after the switch round-trips, the enum row still offers a working \
         Revert (the entry lives at the variant child path, not the row's own)"
    );

    // Add a path entry with server-built defaults, then pull the new row.
    let entry_address = child_address(&mapping_address, "mapping.PathPoints.paths[0]");
    handle.tx.send(ensure_present_action(entry_address.clone()));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view
        .try_recv()
        .expect("entry add + refresh emit a snapshot");
    let entry = find_slot(&snapshot, "mapping.PathPoints.paths[0]");
    assert_eq!(
        entry.state.dirty,
        UiNodeDirtyState::Dirty,
        "the added entry row exists with a server-built default and reads dirty"
    );
    assert_eq!(editor_dirty(&snapshot), (2, 0));

    // Remove it again: add-then-remove cancels on the server (D2). Between
    // the normalized ack and the refresh, the stale view still shows the
    // row — it must read Saving (the AwaitingRefresh bridge), not flash a
    // clean row that then vanishes.
    handle.tx.send(remove_value_action(entry_address));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("entry remove emits a snapshot");
    let entry = find_slot(&snapshot, "mapping.PathPoints.paths[0]");
    assert_eq!(
        entry.state.dirty,
        UiNodeDirtyState::Saving,
        "the normalized removal keeps the Saving treatment until the view catches up"
    );

    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view
        .try_recv()
        .expect("entry remove + refresh emit a snapshot");
    assert!(
        try_find_slot(&snapshot, "mapping.PathPoints.paths[0]").is_none(),
        "the removed entry has no surviving row"
    );
    assert_eq!(
        editor_dirty(&snapshot),
        (1, 0),
        "only the variant switch remains"
    );

    // Revert the variant switch from the enum row itself, exactly as the UI
    // would: dispatch Revert at the row's projected `edit_entry_address`.
    // The overlay empties and the project is clean again, back on the
    // authored Unset variant.
    let mapping = find_slot(&snapshot, "mapping");
    let row_revert_target = mapping
        .edit_entry_address
        .clone()
        .expect("the enum row offers a revert target for the pending switch");
    assert_eq!(row_revert_target, variant_address);
    handle.tx.send(revert_action(row_revert_target));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("revert + refresh emit a snapshot");
    let mapping = find_slot(&snapshot, "mapping");
    assert_eq!(mapping.state.dirty, UiNodeDirtyState::Clean);
    assert_eq!(mapping.detail.as_deref(), Some("variant Unset"));
    assert_eq!(
        editor_dirty(&snapshot),
        (0, 0),
        "the gesture cycle ends clean"
    );
}

#[test]
fn variant_dropdown_switch_away_and_back_ends_clean_from_acks_alone() {
    // The dropdown repro: switch the mapping enum away from its base variant
    // (EnsurePresent mapping.PathPoints), then re-select the base variant
    // (EnsurePresent mapping.Unset). The switch-back normalizes away on the
    // server *and* clears the pending sibling switch; the Materialized ack
    // is the mirror's only source — no ReadOverlay may fire. Without the
    // sibling clearing, the stored mapping.PathPoints entry would survive
    // and the dropdown would stay stuck on PathPoints forever.
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::clone(&sent),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    let mapping = find_slot(&snapshot, "mapping");
    assert_eq!(mapping.detail.as_deref(), Some("variant Unset"));
    assert_eq!(mapping.state.dirty, UiNodeDirtyState::Clean);
    let mapping_address = mapping
        .address
        .clone()
        .expect("mapping slot carries an address");
    let overlay_reads_before = count_overlay_reads(&sent);

    // Switch away, then refresh so the user-visible dropdown really shows
    // the pending variant before switching back.
    handle.tx.send(ensure_present_action(child_address(
        &mapping_address,
        "mapping.PathPoints",
    )));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("switch + refresh emit a snapshot");
    let mapping = find_slot(&snapshot, "mapping");
    assert_eq!(mapping.detail.as_deref(), Some("variant PathPoints"));
    assert_eq!(mapping.state.dirty, UiNodeDirtyState::Dirty);
    assert_eq!(editor_dirty(&snapshot), (1, 0));

    // Re-select the base variant from the dropdown: the pending switch must
    // go away entirely, not normalize into a stuck sibling entry.
    handle.tx.send(ensure_present_action(child_address(
        &mapping_address,
        "mapping.Unset",
    )));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view
        .try_recv()
        .expect("switch-back + refresh emit a snapshot");
    let mapping = find_slot(&snapshot, "mapping");
    assert_eq!(
        mapping.detail.as_deref(),
        Some("variant Unset"),
        "the effective def is back on the base variant"
    );
    assert_eq!(
        mapping.state.dirty,
        UiNodeDirtyState::Clean,
        "no pending sibling switch survives the switch-back"
    );
    assert_eq!(editor_dirty(&snapshot), (0, 0), "the cycle ends clean");
    assert_eq!(
        count_overlay_reads(&sent) - overlay_reads_before,
        0,
        "the mirror is corrected by the ack effects alone, not a ReadOverlay"
    );
}

#[test]
fn option_toggle_off_then_on_ends_clean_from_acks_alone() {
    // The dead-click repro on the fixture `brightness` option (base-present:
    // the shape default is Some(64)): toggle OFF (RemoveValue brightness —
    // stores `Remove` at the option path), refresh, toggle back ON
    // (EnsurePresent brightness.some — normalizes away against base at a
    // DIFFERENT path). The counteracting-entry sweep clears the stored
    // Remove and the Materialized ack is the mirror's only source — no
    // ReadOverlay may fire. Without it, the stored Remove survives and the
    // toggle-on click does nothing, forever.
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::clone(&sent),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    let brightness = find_slot(&snapshot, "brightness");
    assert_eq!(brightness.state.dirty, UiNodeDirtyState::Clean);
    assert_eq!(
        slot_value_display(brightness),
        "64",
        "base default is Some(64)"
    );
    let brightness_address = brightness
        .address
        .clone()
        .expect("brightness slot carries an address");
    assert_eq!(editor_dirty(&snapshot), (0, 0));
    let overlay_reads_before = count_overlay_reads(&sent);

    // Toggle off, then refresh so the user-visible row really shows the
    // excluded state before toggling back on.
    handle
        .tx
        .send(remove_value_action(brightness_address.clone()));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view
        .try_recv()
        .expect("toggle-off + refresh emit a snapshot");
    let brightness = find_slot(&snapshot, "brightness");
    assert!(
        matches!(brightness.body, UiConfigSlotBody::Empty),
        "the toggled-off option row has no value body"
    );
    assert_eq!(brightness.state.dirty, UiNodeDirtyState::Dirty);
    assert_eq!(editor_dirty(&snapshot), (1, 0));

    // Toggle back on: the EnsurePresent at brightness.some normalizes away
    // and must clear the stored Remove at the option path — the exact user
    // symptom was this click doing nothing.
    handle.tx.send(ensure_present_action(child_address(
        &brightness_address,
        "brightness.some",
    )));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view
        .try_recv()
        .expect("toggle-on + refresh emit a snapshot");
    let brightness = find_slot(&snapshot, "brightness");
    assert_eq!(
        slot_value_display(brightness),
        "64",
        "the effective option is back to the base value"
    );
    assert_eq!(
        brightness.state.dirty,
        UiNodeDirtyState::Clean,
        "no counteracting Remove survives the off-then-on cycle"
    );
    assert_eq!(editor_dirty(&snapshot), (0, 0), "the cycle ends clean");
    assert_eq!(
        count_overlay_reads(&sent) - overlay_reads_before,
        0,
        "the mirror is corrected by the ack effects alone, not a ReadOverlay"
    );
}

#[test]
fn removing_an_added_and_edited_entry_ends_clean_from_the_ack_alone() {
    // Mirror fidelity for the subtree-clearing structural remove: add a map
    // entry, edit a leaf under it, remove the entry again. The remove
    // normalizes away on the server and also clears the stranded descendant
    // assignment; the ack (`MutationEffect::Materialized` listing every
    // removed overlay entry) is the mirror's only source — no ReadOverlay
    // may fire. If either side kept the stranded edit, re-applying it would
    // resurrect the entry and the project could never read clean again.
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::clone(&sent),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    let mapping = find_slot(&snapshot, "mapping");
    let mapping_address = mapping
        .address
        .clone()
        .expect("mapping slot carries an address");
    assert_eq!(editor_dirty(&snapshot), (0, 0));
    let overlay_reads_before = count_overlay_reads(&sent);

    // Switch the variant, add an entry, edit a leaf under the added entry.
    let variant_address = child_address(&mapping_address, "mapping.PathPoints");
    handle
        .tx
        .send(ensure_present_action(variant_address.clone()));
    drive(actor.run_one_batch_for_test());
    let entry_address = child_address(&mapping_address, "mapping.PathPoints.paths[0]");
    handle.tx.send(ensure_present_action(entry_address.clone()));
    drive(actor.run_one_batch_for_test());
    let leaf_address = child_address(
        &mapping_address,
        "mapping.PathPoints.paths[0].PointList.first_channel",
    );
    handle
        .tx
        .send(set_value_action(leaf_address, LpValue::U32(7)));
    drive(actor.run_one_batch_for_test());

    // Remove the entry again: the server clears the entry *and* the
    // stranded leaf edit, and the mirror follows from the Materialized ack.
    handle.tx.send(remove_value_action(entry_address));
    drive(actor.run_one_batch_for_test());

    // Revert the remaining variant switch: with the subtree really gone on
    // both sides this empties the overlay entirely.
    handle.tx.send(revert_action(variant_address));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("refresh emits a snapshot");
    let mapping = find_slot(&snapshot, "mapping");
    assert_eq!(mapping.detail.as_deref(), Some("variant Unset"));
    assert_eq!(
        mapping.state.dirty,
        UiNodeDirtyState::Clean,
        "no stranded edit may keep the mapping dirty or resurrect the entry"
    );
    assert!(
        try_find_slot(&snapshot, "mapping.PathPoints.paths[0]").is_none(),
        "the removed entry has no surviving row"
    );
    assert_eq!(editor_dirty(&snapshot), (0, 0), "the cycle ends clean");
    assert_eq!(
        count_overlay_reads(&sent) - overlay_reads_before,
        0,
        "the mirror is corrected by the ack effects alone, not a ReadOverlay"
    );
}

// --- Harness ---------------------------------------------------------------

#[test]
fn shader_asset_editor_fetch_apply_save_and_revert_end_to_end() {
    let server = Rc::new(RefCell::new(asset_e2e_server()));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::clone(&sent),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");

    // The shader node's editor tab exists but its content is unresolved
    // until the editor dispatches the fetch (base bodies are not pulled
    // eagerly for every asset in the project).
    let tab = find_asset_editor(&snapshot);
    assert_eq!(tab.source, "shader.glsl");
    assert!(tab.content.is_none(), "content resolves only on fetch");

    // Fetch → the effective content is the base file body, clean.
    handle.tx.send(StudioCommand::Action(tab.fetch_action()));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("fetch emits a snapshot");
    let tab = find_asset_editor(&snapshot);
    let content = tab.content.as_ref().expect("fetched content");
    assert!(!content.dirty);
    assert_eq!(content.text(), Some(ASSET_SHADER_V1));
    assert_eq!(editor_dirty(&snapshot), (0, 0));

    // Apply an edited body: overlay-backed dirty (persisted-class), the
    // effective content shadows to the applied text, save panel lists it.
    handle
        .tx
        .send(StudioCommand::Action(tab.apply_action(ASSET_SHADER_V2)));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("apply emits a snapshot");
    let tab = find_asset_editor(&snapshot);
    let content = tab.content.as_ref().expect("applied content");
    assert!(content.dirty, "applied body is overlay-dirty");
    assert_eq!(content.text(), Some(ASSET_SHADER_V2));
    assert_eq!(
        editor_dirty(&snapshot),
        (1, 0),
        "asset edits are persisted-class"
    );

    // Save: the .glsl on disk gains the applied source and dirty clears.
    handle.tx.send(project_action(ProjectOp::SaveOverlay));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("save + refresh emit a snapshot");
    let saved = read_project_file(&server, "shader.glsl");
    assert!(
        saved.contains("v2marker"),
        "shader.glsl gained the applied body: {saved}"
    );
    assert_eq!(editor_dirty(&snapshot), (0, 0));

    // The save invalidated the cached base body; the editor re-fetches and
    // sees the committed text, clean.
    let tab = find_asset_editor(&snapshot);
    assert!(tab.content.is_none(), "save invalidates the cached body");
    handle.tx.send(StudioCommand::Action(tab.fetch_action()));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("re-fetch emits a snapshot");
    let tab = find_asset_editor(&snapshot);
    let content = tab.content.as_ref().expect("re-fetched content");
    assert!(!content.dirty);
    assert_eq!(content.text(), Some(ASSET_SHADER_V2));

    // Apply again, then per-entry revert: the overlay entry clears, dirty
    // returns to zero, and the re-fetched content is the saved body.
    handle
        .tx
        .send(StudioCommand::Action(tab.apply_action(ASSET_SHADER_V3)));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("second apply emits a snapshot");
    assert_eq!(editor_dirty(&snapshot), (1, 0));
    let tab = find_asset_editor(&snapshot);
    let revert = UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        crate::AssetEditOp::Revert {
            artifact: tab.artifact.clone(),
        },
    );
    handle.tx.send(StudioCommand::Action(revert));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("revert emits a snapshot");
    assert_eq!(editor_dirty(&snapshot), (0, 0));
    let tab = find_asset_editor(&snapshot);
    handle.tx.send(StudioCommand::Action(tab.fetch_action()));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("post-revert fetch emits a snapshot");
    let tab = find_asset_editor(&snapshot);
    let content = tab.content.as_ref().expect("post-revert content");
    assert!(!content.dirty);
    assert_eq!(
        content.text(),
        Some(ASSET_SHADER_V2),
        "revert returns to the saved body, not the pre-save one"
    );
}

#[test]
fn successive_shader_applies_each_reach_the_engine() {
    // Regression: an overlay→overlay body change (second Apply before any
    // Save) must recompile just like the first (base→overlay) one. Observed
    // live 2026-07-06: the engine kept reporting the first apply's compile
    // error after later applies.
    let server = Rc::new(RefCell::new(asset_e2e_server()));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::clone(&sent),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    let tab = find_asset_editor(&snapshot);

    // First apply: an unknown identifier. Frames advance between edits in
    // production; mirror that here — the mutation must stamp a revision
    // strictly newer than the last compile's, and the engine compiles
    // lazily on render, so tick before and after.
    server.borrow_mut().advance_frame(16).expect("tick");
    handle.tx.send(StudioCommand::Action(tab.apply_action(
        "vec4 render(vec2 pos) { return vec4(first_bad, 0.0, 0.0, 1.0); }",
    )));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv();
    server.borrow_mut().advance_frame(16).expect("tick");
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("refresh emits a snapshot");
    let error = find_asset_editor(&snapshot)
        .shader_error
        .expect("first bad apply surfaces a compile error");
    assert!(
        error.raw.contains("first_bad"),
        "engine error names the first bad identifier: {}",
        error.raw
    );

    // Second apply while the first is still pending in the overlay: the new
    // body must recompile and the error must move to the new identifier.
    let snapshot_tab = find_asset_editor(&snapshot);
    server.borrow_mut().advance_frame(16).expect("tick");
    handle
        .tx
        .send(StudioCommand::Action(snapshot_tab.apply_action(
            "vec4 render(vec2 pos) { return vec4(second_bad, 0.0, 0.0, 1.0); }",
        )));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv();
    server.borrow_mut().advance_frame(16).expect("tick");
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("second refresh emits a snapshot");
    let error = find_asset_editor(&snapshot)
        .shader_error
        .expect("second bad apply surfaces a compile error");
    assert!(
        error.raw.contains("second_bad"),
        "the second applied body reached the engine: {}",
        error.raw
    );

    // And a valid third apply recovers: the error clears.
    let snapshot_tab = find_asset_editor(&snapshot);
    server.borrow_mut().advance_frame(16).expect("tick");
    handle.tx.send(StudioCommand::Action(
        snapshot_tab.apply_action(ASSET_SHADER_V1),
    ));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv();
    server.borrow_mut().advance_frame(16).expect("tick");
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("third refresh emits a snapshot");
    assert_eq!(
        find_asset_editor(&snapshot).shader_error,
        None,
        "a valid re-apply clears the compile error"
    );
}

const ASSET_SHADER_V1: &str =
    "uniform float time;\n\nvec4 render(vec2 pos) {\n    return vec4(pos.x, pos.y, 0.5, 1.0);\n}\n";
const ASSET_SHADER_V2: &str = "// v2marker\nuniform float time;\n\nvec4 render(vec2 pos) {\n    return vec4(pos.y, pos.x, 0.25, 1.0);\n}\n";
const ASSET_SHADER_V3: &str = "// v3marker\nuniform float time;\n\nvec4 render(vec2 pos) {\n    return vec4(0.1, 0.2, 0.3, 1.0);\n}\n";

/// Find the shader node's asset editor tab anywhere in the editor DTO tree
/// (root node tabs or child-node projections).
fn find_asset_editor(view: &UiStudioView) -> crate::UiAssetEditorTab {
    let editor = view
        .panes
        .iter()
        .find_map(|pane| match &pane.body {
            UiViewContent::ProjectEditor(editor) => Some(editor),
            _ => None,
        })
        .expect("project editor pane");

    fn in_children(children: &[crate::UiNodeChild]) -> Option<crate::UiAssetEditorTab> {
        children.iter().find_map(|child| {
            child
                .editor
                .clone()
                .or_else(|| in_children(&child.children))
        })
    }

    editor
        .nodes
        .iter()
        .find_map(|node| {
            node.tabs
                .iter()
                .find_map(|tab| match &tab.body {
                    UiNodeTabBody::AssetEditor(editor) => Some(editor.clone()),
                    _ => None,
                })
                .or_else(|| in_children(&node.children))
        })
        .expect("shader node exposes an asset editor tab")
}

fn asset_e2e_server() -> LpServer {
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: Arc<dyn LpGraphics> = Arc::new(Graphics::new());
    let mut server = LpServer::new(
        output_provider,
        Box::new(LpFsMemory::new()),
        "projects".as_path(),
        None,
        None,
        graphics,
    );

    // The shader publishes to the visual bus and a fixture consumes it —
    // without a consumer the shader never renders, so it would never
    // (re)compile and compile errors would never surface.
    let shader_json = r#"{
  "kind": "Shader",
  "source": "shader.glsl",
  "bindings": {
    "output": { "target": "bus#visual.out" }
  },
  "consumed": {
    "time": {
      "kind": "value",
      "value": "f32",
      "default": 0,
      "label": "Time",
      "description": "Project clock time in seconds"
    }
  }
}"#;
    let fixture_json = r#"{
  "kind": "Fixture",
  "render_size": { "width": 4, "height": 4 },
  "bindings": {
    "input": { "source": "bus#visual.out" },
    "output": { "target": "bus#control.out" }
  }
}"#;
    // The output node drives the demand chain (output pulls control →
    // fixture pulls visual → shader renders/compiles); the memory output
    // provider accepts any authored endpoint.
    let output_json = r#"{
  "kind": "Output",
  "endpoint": "ws281x:rmt:D10",
  "bindings": {
    "input": { "source": "bus#control.out" }
  }
}"#;
    let project_json = r#"{
  "kind": "Project",
  "format": 1,
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "shader": { "ref": "./shader.json" },
    "pixels": { "ref": "./fixture.json" },
    "output": { "ref": "./output.json" }
  }
}"#;
    let clock_json = r#"{
  "kind": "Clock",
  "controls": {
    "running": true,
    "rate": 1.0
  }
}"#;
    let files: &[(&str, &str)] = &[
        ("project.json", project_json),
        ("clock.json", clock_json),
        ("shader.json", shader_json),
        ("fixture.json", fixture_json),
        ("output.json", output_json),
        ("shader.glsl", ASSET_SHADER_V1),
    ];
    for (name, body) in files {
        server
            .base_fs_mut()
            .write_file(format!("{PROJECT_DIR}/{name}").as_path(), body.as_bytes())
            .expect("write project file");
    }
    server
        .load_project(PROJECT_DIR.as_path())
        .expect("load asset-e2e project");
    server.advance_frame(16).expect("tick");
    server
}

const PROJECT_DIR: &str = "/projects/edit-e2e";

/// A real server with a loaded clock + fixture project (no shader, so the
/// simulator session runs entirely host-side).
fn edit_e2e_server() -> LpServer {
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: Arc<dyn LpGraphics> = Arc::new(Graphics::new());
    let mut server = LpServer::new(
        output_provider,
        Box::new(LpFsMemory::new()),
        "projects".as_path(),
        None,
        None,
        graphics,
    );

    let files: &[(&str, &str)] = &[
        (
            "project.json",
            r#"{
  "kind": "Project",
  "format": 1,
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "pixels": { "ref": "./fixture.json" }
  }
}"#,
        ),
        (
            "clock.json",
            r#"{
  "kind": "Clock",
  "controls": {
    "running": true,
    "rate": 1.0
  }
}"#,
        ),
        (
            "fixture.json",
            r#"{
  "kind": "Fixture",
  "render_size": { "width": 10, "height": 10 },
  "bindings": {
    "input": { "source": "bus#visual.out" },
    "output": { "target": "bus#control.out" }
  }
}"#,
        ),
    ];
    for (name, body) in files {
        server
            .base_fs_mut()
            .write_file(format!("{PROJECT_DIR}/{name}").as_path(), body.as_bytes())
            .expect("write project file");
    }
    server
        .load_project(PROJECT_DIR.as_path())
        .expect("load edit-e2e project");
    server.advance_frame(16).expect("tick");
    server
}

fn read_project_file(server: &Rc<RefCell<LpServer>>, name: &str) -> String {
    let bytes = server
        .borrow()
        .base_fs()
        .read_file(format!("{PROJECT_DIR}/{name}").as_path())
        .expect("read project file");
    // Normalize whitespace so assertions are formatting-independent.
    String::from_utf8(bytes)
        .expect("utf8 project file")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn project_action(op: ProjectOp) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        op,
    ))
}

fn set_value_action(address: crate::ProjectSlotAddress, value: LpValue) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        SlotEditOp::SetValue { address, value },
    ))
}

fn revert_action(address: crate::ProjectSlotAddress) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        SlotEditOp::Revert { address },
    ))
}

fn ensure_present_action(address: crate::ProjectSlotAddress) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        SlotEditOp::EnsurePresent { address },
    ))
}

fn remove_value_action(address: crate::ProjectSlotAddress) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        SlotEditOp::RemoveValue { address },
    ))
}

/// An address under the same node and slot root as `base`, at `path`.
fn child_address(base: &crate::ProjectSlotAddress, path: &str) -> crate::ProjectSlotAddress {
    crate::ProjectSlotAddress::new(
        base.node.clone(),
        base.root.clone(),
        SlotPath::parse(path).unwrap(),
    )
}

fn count_mutations(sent: &Rc<RefCell<Vec<ClientMessage>>>) -> usize {
    sent.borrow()
        .iter()
        .filter(|message| {
            matches!(
                &message.msg,
                ClientRequest::ProjectCommand {
                    command: WireProjectCommand::MutateOverlay { .. },
                    ..
                }
            )
        })
        .count()
}

fn count_overlay_reads(sent: &Rc<RefCell<Vec<ClientMessage>>>) -> usize {
    sent.borrow()
        .iter()
        .filter(|message| {
            matches!(
                &message.msg,
                ClientRequest::ProjectCommand {
                    command: WireProjectCommand::ReadOverlay { .. },
                    ..
                }
            )
        })
        .count()
}

fn editor_dirty(view: &UiStudioView) -> (usize, usize) {
    let editor = view
        .panes
        .iter()
        .find_map(|pane| match &pane.body {
            UiViewContent::ProjectEditor(editor) => Some(editor),
            _ => None,
        })
        .expect("project editor pane");
    (editor.dirty.persisted, editor.dirty.transient)
}

/// Find a config slot anywhere in the editor DTO tree by its address path.
fn find_slot<'a>(view: &'a UiStudioView, path: &str) -> &'a UiConfigSlot {
    try_find_slot(view, path).unwrap_or_else(|| panic!("config slot with path {path} should exist"))
}

/// Like [`find_slot`], but `None` when no row carries the address path.
fn try_find_slot<'a>(view: &'a UiStudioView, path: &str) -> Option<&'a UiConfigSlot> {
    let editor = view
        .panes
        .iter()
        .find_map(|pane| match &pane.body {
            UiViewContent::ProjectEditor(editor) => Some(editor),
            _ => None,
        })
        .expect("project editor pane");

    fn in_slots<'a>(slots: &'a [UiConfigSlot], path: &str) -> Option<&'a UiConfigSlot> {
        for slot in slots {
            if slot
                .address
                .as_ref()
                .is_some_and(|address| address.path.to_string() == path)
            {
                return Some(slot);
            }
            if let UiConfigSlotBody::Record(record) = &slot.body
                && let Some(found) = in_slots(&record.fields, path)
            {
                return Some(found);
            }
        }
        None
    }

    fn in_sections<'a>(sections: &'a [UiNodeSection], path: &str) -> Option<&'a UiConfigSlot> {
        sections.iter().find_map(|section| match section {
            UiNodeSection::ConfigSlots(slots) | UiNodeSection::AssetSlots(slots) => {
                in_slots(slots, path)
            }
            _ => None,
        })
    }

    fn in_children<'a>(children: &'a [crate::UiNodeChild], path: &str) -> Option<&'a UiConfigSlot> {
        children.iter().find_map(|child| {
            in_sections(&child.sections, path).or_else(|| in_children(&child.children, path))
        })
    }

    editor.nodes.iter().find_map(|node| {
        node.tabs
            .iter()
            .find_map(|tab| match &tab.body {
                UiNodeTabBody::Sections(sections) => in_sections(sections, path),
                _ => None,
            })
            .or_else(|| in_children(&node.children, path))
    })
}

fn slot_value_display(slot: &UiConfigSlot) -> &str {
    let UiConfigSlotBody::Value(value) = &slot.body else {
        panic!("expected a value body for {}", slot.label);
    };
    &value.display
}

/// `ClientIo` that pumps each client message through the in-process server's
/// `tick_and_send` and queues the produced frames for `receive`.
struct InProcessServerIo {
    server: Rc<RefCell<LpServer>>,
    inbox: Rc<RefCell<VecDeque<WireServerMessage>>>,
    sent: Rc<RefCell<Vec<ClientMessage>>>,
}

impl ClientIo for InProcessServerIo {
    fn send<'life0, 'async_trait>(
        &'life0 mut self,
        msg: ClientMessage,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        self.sent.borrow_mut().push(msg.clone());
        let server = Rc::clone(&self.server);
        let inbox = Rc::clone(&self.inbox);
        Box::pin(async move {
            let mut transport = CollectTransport::default();
            server
                .borrow_mut()
                .tick_and_send(16, vec![WireMessage::Client(msg)], &mut transport)
                .await
                .map_err(|error| TransportError::Other(format!("server error: {error}")))?;
            inbox.borrow_mut().extend(transport.sent);
            Ok(())
        })
    }

    fn receive<'life0, 'async_trait>(
        &'life0 mut self,
    ) -> Pin<Box<dyn Future<Output = Result<WireServerMessage, TransportError>> + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        let response = self
            .inbox
            .borrow_mut()
            .pop_front()
            .ok_or_else(|| TransportError::Other("in-process server inbox empty".to_string()));
        Box::pin(async move { response })
    }

    fn close<'life0, 'async_trait>(
        &'life0 mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async { Ok(()) })
    }
}

/// In-memory server transport that records every sent frame.
#[derive(Default)]
struct CollectTransport {
    sent: Vec<WireServerMessage>,
}

impl ServerTransport for CollectTransport {
    async fn send(&mut self, msg: WireServerMessage) -> Result<(), TransportError> {
        self.sent.push(msg);
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        Ok(None)
    }

    async fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError> {
        Ok(Vec::new())
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

/// Drive a future to completion with a self-waking waker (bounded, so a hung
/// future fails the test instead of the suite).
fn drive<F: Future>(future: F) -> F::Output {
    struct NoopWake;
    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    for _ in 0..100_000 {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => {}
        }
    }
    panic!("e2e future did not complete");
}
