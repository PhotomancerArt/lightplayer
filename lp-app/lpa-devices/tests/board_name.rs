//! One board name (2026-10-28): a person's rename is the board's own name
//! too. The model writes it to the board when the board can take it, keeps
//! it unsent until the board says it has it, and otherwise follows the name
//! the board reports.

use lpa_devices::link::LinkEvent;
use lpa_devices::replay::{Replay, Step};
use lpa_devices::wire::{ClientFrameBody, ServerFrame};
use lpa_devices::{
    Action, Command, DeviceId, Event, Input, LinkCommand, LinkId, Millis, RosterConfig,
};

#[test]
fn a_rename_on_a_current_board_is_written_to_it_and_confirmed() {
    let mut replay = ready_device();
    let device = first_device(&replay);

    let commands = replay.feed(Millis(1_000), rename(device, "Flower-Viatrix"));
    assert_eq!(sent_names(&commands), ["Flower-Viatrix"]);
    let entry = &replay.roster().devices()[0];
    assert_eq!(entry.intent.name.as_deref(), Some("Flower-Viatrix"));
    assert!(entry.intent.name_unsent, "unsent until the board answers");

    replay.feed(Millis(1_100), named(Some("Flower-Viatrix")));
    let entry = &replay.roster().devices()[0];
    assert!(!entry.intent.name_unsent, "the board has it");
    assert_eq!(entry.identity.name.as_deref(), Some("Flower-Viatrix"));
    assert_eq!(entry.title(), "Flower-Viatrix");
}

#[test]
fn an_older_board_is_never_sent_the_name_and_gets_it_once_current() {
    let config = RosterConfig {
        expected_proto: 47,
        ..RosterConfig::default()
    };
    let mut replay = Replay::new(config);
    replay.step(Millis(0), Step::attach(1, "usb-1"));
    replay.step(Millis(20), Step::opened(1));
    replay.step(Millis(200), Step::hello(1).uid("dev_2f8a").proto(46));
    let device = first_device(&replay);

    let commands = replay.feed(Millis(1_000), rename(device, "Porch"));
    assert!(sent_names(&commands).is_empty(), "wire 46 does not know it");
    assert!(replay.roster().devices()[0].intent.name_unsent);

    // The board is updated: its next hello is at this Studio's wire.
    replay.step(Millis(2_000), Step::detach(1));
    replay.step(Millis(3_000), Step::attach(2, "usb-1"));
    replay.step(Millis(3_020), Step::opened(2));
    let commands = replay.step(Millis(3_200), Step::hello(2).uid("dev_2f8a").proto(47));
    assert_eq!(sent_names(&commands), ["Porch"]);
}

#[test]
fn a_name_the_board_reports_is_the_title_when_nothing_is_unsent() {
    let mut replay = Replay::new(RosterConfig::default());
    replay.step(Millis(0), Step::attach(1, "usb-1"));
    replay.step(Millis(20), Step::opened(1));
    let device_name = "Renamed elsewhere";
    let commands = replay.step(
        Millis(200),
        Step::hello(1).uid("dev_2f8a").named(device_name),
    );
    assert!(sent_names(&commands).is_empty());
    let entry = &replay.roster().devices()[0];
    assert_eq!(entry.title(), device_name);
    assert!(!entry.intent.name_unsent);
}

#[test]
fn studios_own_name_is_never_written_to_the_board() {
    let mut replay = ready_device();
    let device = first_device(&replay);
    let commands = replay.feed(
        Millis(1_000),
        Input::Action(Action::SetName {
            device,
            name: "XIAO ESP32-C6 · Oct 28".to_string(),
        }),
    );
    assert!(sent_names(&commands).is_empty());
    assert!(!replay.roster().devices()[0].intent.name_unsent);
}

#[test]
fn an_unsent_rename_wins_over_the_name_the_board_still_reports() {
    let mut replay = Replay::new(RosterConfig::default());
    replay.step(Millis(0), Step::attach(1, "usb-1"));
    replay.step(Millis(20), Step::opened(1));
    replay.step(Millis(200), Step::hello(1).uid("dev_2f8a").named("Old"));
    let device = first_device(&replay);
    replay.step(Millis(500), Step::detach(1));
    // Renamed while the board is away.
    let commands = replay.feed(Millis(1_000), rename(device, "New"));
    assert!(sent_names(&commands).is_empty(), "no link to send it on");

    replay.step(Millis(2_000), Step::attach(2, "usb-1"));
    replay.step(Millis(2_020), Step::opened(2));
    let commands = replay.step(Millis(2_200), Step::hello(2).uid("dev_2f8a").named("Old"));
    assert_eq!(sent_names(&commands), ["New"]);
    assert_eq!(replay.roster().devices()[0].title(), "New");
}

fn ready_device() -> Replay {
    let mut replay = Replay::new(RosterConfig::default());
    replay.step(Millis(0), Step::attach(1, "usb-1"));
    replay.step(Millis(20), Step::opened(1));
    replay.step(Millis(200), Step::hello(1).uid("dev_2f8a").board("dig-uno"));
    assert_eq!(replay.roster().devices().len(), 1, "setup: one device");
    replay
}

fn first_device(replay: &Replay) -> DeviceId {
    replay.roster().devices()[0].id
}

fn rename(device: DeviceId, name: &str) -> Input {
    Input::Action(Action::NameBoard {
        device,
        name: name.to_string(),
    })
}

fn named(name: Option<&str>) -> Input {
    Input::Event(Event::Link {
        link: LinkId(1),
        event: LinkEvent::Frame(ServerFrame::named(3, name.map(str::to_string))),
    })
}

fn sent_names(commands: &[Command]) -> Vec<String> {
    commands
        .iter()
        .filter_map(|command| match command {
            Command::Link {
                command: LinkCommand::SendFrame(frame),
                ..
            } => match &frame.body {
                ClientFrameBody::SetDeviceName { name } => Some(name.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}
