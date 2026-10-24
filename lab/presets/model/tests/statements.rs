//! Statements: how presets, activations and schedules behave, one sentence
//! per test.
//!
//! Each test's doc comment IS the statement. Read this file as the list.
//! When a statement changes, change the sentence first, then the code.
//! The R-numbers are the requirements in the planning notes
//! (`lp2025/2026-10-17-0305-wall-clock-schedules/notes.md`).
//!
//! The show is `Show::eaves()` (see `src/seed.rs`): panel defaults keep the
//! lights off at 80 % brightness, and four schedules turn presets on —
//! Evenings (sunset − 15 until 00:30 → Evening), Late nights (23:00 for
//! 90 min → Late dim), Halloween nights (Oct 1–31, 18:30 until 23:00 →
//! Halloween) and Christmas season (Dec 1 – Jan 6, sunset until midnight →
//! Christmas). Priority, bottom first: Evening, Late dim, Christmas,
//! Halloween, Party. Dates are 2026 unless they say otherwise; Halloween is
//! a Saturday.

use lab_presets_model::sun;
use lab_presets_model::*;

// ---- R1: on and off by the clock and the sun ------------------------------

/// The lights come on fifteen minutes before sunset, because Evening pins
/// power on.
#[test]
fn evening_turns_the_lights_on_fifteen_minutes_before_sunset() {
    let mut show = Show::eaves();
    let day = at(10, 10, 0, 0).day();
    let on = day.start().plus(sun::sunset(day) - 15);
    show.tick(Some(on.plus(-1)));
    assert_eq!(show.value("power").value, Value::On(false));
    assert_eq!(show.value("power").layer, Layer::PanelDefaults);
    show.tick(Some(on));
    assert_eq!(show.value("power").value, Value::On(true));
    assert_eq!(
        show.value("power").layer,
        Layer::Preset(show.preset_named("Evening"))
    );
}

/// The lights go off at half past midnight, when Evening ends and the panel
/// defaults show through again.
#[test]
fn the_lights_go_off_at_half_past_midnight() {
    let mut show = Show::eaves();
    show.tick(Some(at(11, 3, 20, 0)));
    show.tick(Some(at(11, 4, 0, 29)));
    assert_eq!(show.value("power").value, Value::On(true));
    show.tick(Some(at(11, 4, 0, 30)));
    assert_eq!(show.value("power").value, Value::On(false));
    assert_eq!(show.value("power").layer, Layer::PanelDefaults);
}

/// Late dim pins only brightness: at 23:30 the lights are at 30 % and still
/// cycle Evening's patterns.
#[test]
fn late_dim_pins_brightness_and_leaves_the_pattern_alone() {
    let mut show = Show::eaves();
    show.tick(Some(at(11, 3, 20, 0)));
    show.tick(Some(at(11, 3, 23, 30)));
    assert_eq!(show.value("brightness").value, Value::Level(30));
    assert_eq!(
        show.value("pattern").value,
        Value::Cycle(vec!["Embers", "Aurora", "Twinkle"])
    );
    assert_eq!(
        show.value("pattern").layer,
        Layer::Preset(show.preset_named("Evening"))
    );
}

// ---- R2: patterns by date and time -----------------------------------------

/// On Halloween night the pattern is Halloween's, because Halloween is above
/// Evening in the order; brightness still comes from the panel defaults.
#[test]
fn halloween_wins_the_pattern_over_evening_in_october() {
    let mut show = Show::eaves();
    show.tick(Some(at(10, 31, 19, 0)));
    assert_eq!(
        show.value("pattern").value,
        Value::Cycle(vec!["Spooky", "Pumpkin"])
    );
    assert_eq!(show.value("brightness").value, Value::Level(80));
    assert_eq!(show.value("brightness").layer, Layer::PanelDefaults);
}

/// Move Halloween below Evening and Evening's pattern wins instead, while
/// Halloween's palette (which Evening doesn't pin) still shows.
#[test]
fn moving_halloween_below_evening_gives_evening_the_pattern() {
    let mut show = Show::eaves();
    show.move_preset(show.preset_named("Halloween"), 0);
    show.tick(Some(at(10, 31, 19, 0)));
    assert_eq!(
        show.value("pattern").value,
        Value::Cycle(vec!["Embers", "Aurora", "Twinkle"])
    );
    assert_eq!(show.value("palette").value, Value::Choice("Orange"));
}

/// The Christmas season wraps the new year: it fires on January 3rd and not
/// on January 7th.
#[test]
fn christmas_season_wraps_the_new_year() {
    let mut show = Show::eaves();
    let christmas = show.preset_named("Christmas");
    show.tick(Some(Minute::at(2027, 1, 3, 20, 0)));
    assert!(is_active(&show, christmas));
    let mut show = Show::eaves();
    show.tick(Some(Minute::at(2027, 1, 7, 20, 0)));
    assert!(!is_active(&show, christmas));
}

/// Halloween nights don't fire in November.
#[test]
fn halloween_does_not_fire_in_november() {
    let mut show = Show::eaves();
    show.tick(Some(at(11, 1, 19, 0)));
    assert!(!is_active(&show, show.preset_named("Halloween")));
}

// ---- R3 and R8: the time, and coming back by itself ----------------------

/// Before the board knows the time it shows the panel defaults: a house that
/// keeps power off there fails dark.
#[test]
fn before_the_board_knows_the_time_it_shows_the_panel_defaults() {
    let mut show = Show::eaves();
    show.tick(None);
    assert_eq!(show.value("power").value, Value::On(false));
    assert!(show.active_presets().is_empty());
}

/// To fail bright instead, set power on in the panel defaults.
#[test]
fn a_house_that_fails_bright_sets_power_on_in_its_panel_defaults() {
    let mut show = Show::eaves();
    show.set_panel_default("power", Value::On(true));
    show.tick(None);
    assert_eq!(show.value("power").value, Value::On(true));
}

/// After a power cut the schedule catches up on what it missed: the board
/// back at 19:30 turns Halloween on until 23:00, and says it caught up.
#[test]
fn after_a_power_cut_the_schedule_catches_up_on_what_it_missed() {
    let mut show = Show::eaves();
    show.tick(Some(at(10, 31, 18, 0)));
    show.power_cut();
    show.tick(None);
    show.tick(Some(at(10, 31, 19, 30)));
    let halloween = show.preset_named("Halloween");
    assert_eq!(ends_of(&show, halloween), vec![Some(at(10, 31, 23, 0))]);
    assert!(log_has(&show, "caught up from Sat Oct 31 18:30"));
}

/// A firing that already ended while the power was out is not caught up.
#[test]
fn a_firing_that_ended_while_the_power_was_out_is_not_caught_up() {
    let mut show = Show::eaves();
    show.tick(Some(at(10, 31, 18, 0)));
    show.power_cut();
    show.tick(Some(at(10, 31, 23, 30)));
    assert!(!is_active(&show, show.preset_named("Halloween")));
    assert!(is_active(&show, show.preset_named("Evening")));
    assert_eq!(show.value("brightness").value, Value::Level(30));
}

/// A soft reset (an update's reboot) keeps what you turned on by hand, and it
/// keeps showing while the board relearns the time.
#[test]
fn a_soft_reset_keeps_what_you_turned_on_by_hand() {
    let mut show = Show::eaves();
    let party = show.preset_named("Party");
    show.tick(Some(at(11, 3, 20, 0)));
    show.activate(party, Some(60));
    show.soft_reset();
    show.tick(None);
    assert!(is_active(&show, party));
    show.tick(Some(at(11, 3, 20, 30)));
    assert!(is_active(&show, party));
    show.tick(Some(at(11, 3, 21, 0)));
    assert!(!is_active(&show, party));
}

/// A power cut loses what you turned on by hand.
#[test]
fn a_power_cut_loses_what_you_turned_on_by_hand() {
    let mut show = Show::eaves();
    let party = show.preset_named("Party");
    show.tick(Some(at(11, 3, 20, 0)));
    show.activate(party, Some(60));
    show.power_cut();
    show.tick(Some(at(11, 3, 20, 5)));
    assert!(!is_active(&show, party));
}

// ---- R4: testing --------------------------------------------------------

/// "Pretend it's Halloween at 19:00" is only another time: a board that has
/// run all October and one that wakes then show the same thing.
#[test]
fn pretending_it_is_halloween_is_only_another_time() {
    let mut ran = Show::eaves();
    for day in 1..=30 {
        for hour in [12, 18, 19, 23] {
            ran.tick(Some(at(10, day, hour, 0)));
        }
    }
    ran.tick(Some(at(10, 31, 12, 0)));
    ran.tick(Some(at(10, 31, 19, 0)));
    let mut woke = Show::eaves();
    woke.tick(Some(at(10, 31, 19, 0)));
    assert_eq!(ran.resolve(), woke.resolve());
}

/// Pretending never moves the board's clock: the lights keep showing now,
/// and the pretend copy shows Halloween night.
#[test]
fn pretending_leaves_the_board_alone() {
    let mut show = Show::eaves();
    show.tick(Some(at(11, 3, 12, 0)));
    let pretend = show.pretend(at(10, 31, 19, 0));
    assert_eq!(pretend.value("palette").value, Value::Choice("Orange"));
    assert_eq!(show.value("power").value, Value::On(false));
    assert_eq!(show.clock(), Some(at(11, 3, 12, 0)));
}

/// The calendar for the week of Halloween shows seven Halloween nights.
#[test]
fn the_calendar_shows_every_firing_in_a_window() {
    let show = Show::eaves();
    let nights = &show.saved.schedules[2];
    assert_eq!(
        nights
            .firings_between(at(10, 25, 0, 0), at(11, 1, 0, 0))
            .len(),
        7
    );
}

// ---- R5: seeing and switching schedules ----------------------------------

/// Switching a schedule off stops its next firing but leaves tonight's on.
#[test]
fn switching_a_schedule_off_stops_its_next_firing_but_not_tonights() {
    let mut show = Show::eaves();
    let halloween = show.preset_named("Halloween");
    show.tick(Some(at(10, 30, 19, 0)));
    show.edit_schedule(ScheduleId(3), |s| s.enabled = false);
    show.tick(Some(at(10, 30, 19, 1)));
    assert!(is_active(&show, halloween));
    show.tick(Some(at(10, 31, 19, 0)));
    assert!(!is_active(&show, halloween));
}

/// Editing a schedule never changes what is showing now: moving tonight's
/// start earlier than now does not turn it on until tomorrow.
#[test]
fn editing_a_schedule_never_changes_what_is_showing_now() {
    let mut show = Show::eaves();
    let halloween = show.preset_named("Halloween");
    show.tick(Some(at(10, 10, 18, 0)));
    show.edit_schedule(ScheduleId(3), |s| {
        s.start = TimeOfDay::Clock {
            hour: 17,
            minute: 30,
        }
    });
    show.tick(Some(at(10, 10, 18, 1)));
    assert!(!is_active(&show, halloween));
    show.tick(Some(at(10, 11, 17, 30)));
    assert!(is_active(&show, halloween));
}

// ---- R6: knowing it worked -------------------------------------------------

/// The log says what came on, by which schedule, and until when.
#[test]
fn the_log_says_what_came_on_and_why() {
    let mut show = Show::eaves();
    show.tick(Some(at(10, 31, 18, 30)));
    assert!(log_has(
        &show,
        "Halloween on by \"Halloween nights\", until Sat Oct 31 23:00"
    ));
}

// ---- R7: overriding ---------------------------------------------------------

/// Turning a control lasts until the schedule next changes: brightness set to
/// 50 % at 20:00 gives way to Late dim at 23:00.
#[test]
fn turning_a_control_lasts_until_the_schedule_next_changes() {
    let mut show = Show::eaves();
    show.tick(Some(at(10, 31, 20, 0)));
    show.turn("brightness", Value::Level(50));
    assert_eq!(show.value("brightness").layer, Layer::Overrides);
    assert_eq!(show.live.overrides.ends, Some(at(10, 31, 23, 0)));
    show.tick(Some(at(10, 31, 23, 0)));
    assert_eq!(show.value("brightness").value, Value::Level(30));
}

/// A surprise, kept on purpose until it's decided: "until the schedule next
/// changes" means ANY change, so brightness turned at 18:25 on Halloween ends
/// at 18:30, when Halloween starts — though Halloween doesn't pin brightness.
#[test]
fn a_change_ends_at_the_next_change_even_one_that_does_not_touch_it() {
    let mut show = Show::eaves();
    show.tick(Some(at(10, 31, 18, 25)));
    show.turn("brightness", Value::Level(50));
    assert_eq!(show.live.overrides.ends, Some(at(10, 31, 18, 30)));
    show.tick(Some(at(10, 31, 18, 30)));
    assert_eq!(show.value("brightness").value, Value::Level(80));
}

/// A control turned before the board knows the time gets its end when the
/// time arrives.
#[test]
fn a_change_before_the_time_is_known_gets_its_end_when_it_is() {
    let mut show = Show::eaves();
    show.turn("power", Value::On(true));
    assert_eq!(show.live.overrides.ends, None);
    show.tick(Some(at(11, 3, 12, 0)));
    let sunset_less_15 = at(11, 3, 0, 0).plus(sun::sunset(at(11, 3, 0, 0).day()) - 15);
    assert_eq!(show.live.overrides.ends, Some(sunset_less_15));
}

/// Turning Halloween off tonight leaves Evening's look, and Halloween comes
/// back tomorrow at 18:30.
#[test]
fn turning_halloween_off_tonight_brings_it_back_tomorrow() {
    let mut show = Show::eaves();
    let halloween = show.preset_named("Halloween");
    show.tick(Some(at(10, 30, 20, 0)));
    show.turn_off(halloween);
    show.tick(Some(at(10, 30, 20, 1)));
    assert!(!is_active(&show, halloween));
    assert_eq!(
        show.value("pattern").layer,
        Layer::Preset(show.preset_named("Evening"))
    );
    show.tick(Some(at(10, 31, 18, 30)));
    assert!(is_active(&show, halloween));
}

/// Turned off tonight stays off through a soft reset.
#[test]
fn turned_off_tonight_stays_off_through_a_soft_reset() {
    let mut show = Show::eaves();
    let halloween = show.preset_named("Halloween");
    show.tick(Some(at(10, 30, 20, 0)));
    show.turn_off(halloween);
    show.soft_reset();
    show.tick(Some(at(10, 30, 20, 5)));
    assert!(!is_active(&show, halloween));
}

/// But a power cut forgets it, and catching up turns tonight's Halloween back
/// on. (Accepted: what you did by hand is lost on power off.)
#[test]
fn a_power_cut_forgets_that_you_turned_halloween_off() {
    let mut show = Show::eaves();
    let halloween = show.preset_named("Halloween");
    show.tick(Some(at(10, 30, 20, 0)));
    show.turn_off(halloween);
    show.power_cut();
    show.tick(Some(at(10, 30, 20, 5)));
    assert!(is_active(&show, halloween));
}

/// "Halloween mode today" outlasts the schedule's own Halloween: the manual
/// activation is another source, so the 23:00 end doesn't touch it.
#[test]
fn halloween_mode_today_outlasts_the_schedules_halloween() {
    let mut show = Show::eaves();
    let halloween = show.preset_named("Halloween");
    show.tick(Some(at(10, 15, 9, 0)));
    show.activate(halloween, Some(15 * 60)); // until midnight
    show.tick(Some(at(10, 15, 18, 30)));
    assert_eq!(ends_of(&show, halloween).len(), 2);
    show.tick(Some(at(10, 15, 23, 0)));
    assert!(is_active(&show, halloween));
    show.tick(Some(at(10, 16, 0, 0)));
    assert!(!is_active(&show, halloween));
}

/// Two schedules for one preset make two activations, and they end
/// separately.
#[test]
fn two_schedules_for_one_preset_end_separately() {
    let mut show = Show::eaves();
    let halloween = show.preset_named("Halloween");
    show.add_schedule(Schedule {
        id: ScheduleId(9),
        name: "Halloween weekend".into(),
        preset: halloween,
        enabled: true,
        days: Weekdays::only(&[Weekday::Fri, Weekday::Sat]),
        dates: Some(DateRange {
            from: (10, 1),
            to: (10, 31),
        }),
        start: TimeOfDay::Clock {
            hour: 17,
            minute: 0,
        },
        end: ScheduleEnd::At(TimeOfDay::Clock { hour: 1, minute: 0 }),
        since: Minute::at(2026, 1, 1, 0, 0),
    });
    show.tick(Some(at(10, 31, 19, 0)));
    assert_eq!(
        ends_of(&show, halloween),
        vec![Some(at(11, 1, 1, 0)), Some(at(10, 31, 23, 0))]
    );
    show.tick(Some(at(10, 31, 23, 0)));
    assert_eq!(ends_of(&show, halloween), vec![Some(at(11, 1, 1, 0))]);
}

/// Ending one source's activation leaves the other's.
#[test]
fn ending_one_source_leaves_the_others() {
    let mut show = Show::eaves();
    let halloween = show.preset_named("Halloween");
    show.tick(Some(at(10, 31, 19, 0)));
    show.activate(halloween, None);
    show.end_one(halloween, SourceKey::Schedule(ScheduleId(3)));
    let active = &show.active_presets()[0];
    assert_eq!(active.preset, halloween);
    assert_eq!(active.activations.len(), 1);
    assert_eq!(active.activations[0].source, Source::Manual);
}

// ---- presets ------------------------------------------------------------

/// Saving your changes as a preset changes nothing you see: the values stay,
/// and now the new preset holds them.
#[test]
fn saving_your_changes_as_a_preset_changes_nothing_you_see() {
    let mut show = Show::eaves();
    show.tick(Some(at(10, 31, 20, 0)));
    show.turn("palette", Value::Choice("Ice"));
    show.turn("speed", Value::Level(20));
    let before: Vec<_> = show.resolve().into_iter().map(|r| r.value).collect();
    let frost = show.save_overrides_as_preset("Frost", "#9cc8ee");
    let after: Vec<_> = show.resolve().into_iter().map(|r| r.value).collect();
    assert_eq!(before, after);
    assert_eq!(show.value("palette").layer, Layer::Preset(frost));
    assert!(show.live.overrides.is_empty());
}

/// Editing an active preset shows at once.
#[test]
fn editing_an_active_preset_shows_at_once() {
    let mut show = Show::eaves();
    let halloween = show.preset_named("Halloween");
    show.tick(Some(at(10, 31, 19, 0)));
    show.pin(halloween, "palette", Some(Value::Choice("Purple")));
    assert_eq!(show.value("palette").value, Value::Choice("Purple"));
}

/// Pressing a preset again replaces its end rather than adding another.
#[test]
fn pressing_a_preset_again_replaces_its_end() {
    let mut show = Show::eaves();
    let party = show.preset_named("Party");
    show.tick(Some(at(11, 3, 20, 0)));
    show.activate(party, Some(60));
    show.activate(party, Some(120));
    assert_eq!(ends_of(&show, party), vec![Some(at(11, 3, 22, 0))]);
}

/// Every control says where its value came from. At 23:30 on Halloween:
/// power and pattern from Evening, brightness from Late dim, palette and
/// speed from the project.
#[test]
fn every_control_says_where_its_value_came_from() {
    let mut show = Show::eaves();
    show.tick(Some(at(10, 31, 23, 30)));
    let evening = Layer::Preset(show.preset_named("Evening"));
    let late_dim = Layer::Preset(show.preset_named("Late dim"));
    let layers: Vec<_> = show
        .resolve()
        .into_iter()
        .map(|r| (r.control, r.layer))
        .collect();
    assert_eq!(
        layers,
        vec![
            ("brightness", late_dim),
            ("palette", Layer::Project),
            ("pattern", evening),
            ("power", evening),
            ("speed", Layer::Project),
        ]
    );
}

/// The active presets row lists the top of the stack first.
#[test]
fn the_active_presets_row_lists_the_top_of_the_stack_first() {
    let mut show = Show::eaves();
    show.tick(Some(at(10, 31, 22, 0)));
    let names: Vec<_> = show
        .active_presets()
        .iter()
        .map(|a| show.preset(a.preset).name.clone())
        .collect();
    assert_eq!(names, vec!["Halloween", "Evening"]);
}

// ---- helpers --------------------------------------------------------------

fn at(month: u8, day: u8, hour: u8, minute: u8) -> Minute {
    Minute::at(2026, month, day, hour, minute)
}

fn is_active(show: &Show, preset: PresetId) -> bool {
    show.active_presets().iter().any(|a| a.preset == preset)
}

/// The end of each activation holding `preset`, latest first.
fn ends_of(show: &Show, preset: PresetId) -> Vec<Option<Minute>> {
    let mut ends: Vec<_> = show
        .live
        .activations
        .iter()
        .filter(|a| a.preset == preset)
        .map(Activation::ends)
        .collect();
    ends.sort_by(|a, b| b.cmp(a));
    ends
}

fn log_has(show: &Show, text: &str) -> bool {
    show.live.log.iter().any(|l| l.text.contains(text))
}
