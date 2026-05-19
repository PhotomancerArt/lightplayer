use anyhow::Result;
use dialoguer::Input;

use super::app::{App, PulseStatus, Route, is_serial_disconnect};
use super::calibration_manifest_update::GpioCandidate;
use super::model::{self, LabelStatus};
use super::ui::{BOLD, DIM, GREEN, YELLOW, paint, section, shortcut};

pub fn show(app: &mut App, label: String) -> Result<Route> {
    model::ensure_label(&mut app.manifest, &label)?;
    app.save()?;
    loop {
        let row = model::row(&app.manifest, &label);
        print_pin(&row);
        let prompt = match row.status {
            LabelStatus::Assigned | LabelStatus::Verified => format!(
                "Enter=test assignment, {}, {}, {}, {}",
                shortcut('r', "emap/search"),
                shortcut('u', "nassign"),
                shortcut('b', "oard"),
                shortcut('q', "uit")
            ),
            _ => format!(
                "Enter=search, {}, {}, {}",
                shortcut('s', "kip"),
                shortcut('b', "oard"),
                shortcut('q', "uit")
            ),
        };
        let command: String = Input::new()
            .with_prompt(prompt)
            .allow_empty(true)
            .interact_text()?;
        let command = command.trim().to_ascii_lowercase();
        match command.as_str() {
            "" if matches!(row.status, LabelStatus::Assigned | LabelStatus::Verified) => {
                if let Some(gpio) = row.gpio {
                    test_assignment(app, &label, gpio)?;
                }
            }
            "" => return Ok(Route::Search(label)),
            "r" => return Ok(Route::Search(label)),
            "u" => {
                model::unassign(&mut app.manifest, &label)?;
                app.save()?;
            }
            "s" => {
                model::mark_skipped(&mut app.manifest, &label)?;
                app.save()?;
                return Ok(Route::Board);
            }
            "b" => return Ok(Route::Board),
            "q" => return Ok(Route::Quit),
            _ => println!("Expected Enter, r, u, s, b, or q."),
        }
    }
}

fn print_pin(row: &model::LabelRow) {
    section(&row.label);
    let status = match row.status {
        LabelStatus::Assigned | LabelStatus::Verified => paint(GREEN, row.status.as_str()),
        LabelStatus::NotFound | LabelStatus::Skipped => paint(YELLOW, row.status.as_str()),
        LabelStatus::Unassigned => paint(DIM, row.status.as_str()),
    };
    println!("{} {}", paint(BOLD, "Status:"), status);
    let gpio = row
        .gpio
        .map(|gpio| format!("/gpio/{gpio}"))
        .unwrap_or_else(|| "-".into());
    println!("{} {gpio}", paint(BOLD, "GPIO:"));
    println!();
}

fn test_assignment(app: &mut App, label: &str, gpio: u32) -> Result<()> {
    let candidate = GpioCandidate {
        gpio,
        address: format!("/gpio/{gpio}"),
        display_label: label.to_string(),
    };
    let status = match app.pulse_candidate(&candidate) {
        Err(error) if is_serial_disconnect(&error) => PulseStatus::LostConnection {
            message: error.to_string(),
        },
        Err(error) => return Err(error),
        Ok(status) => status,
    };
    match status {
        PulseStatus::Alive => {
            let answer: String = Input::new()
                .with_prompt(format!("Is {label} active on /gpio/{gpio}? (y/N)"))
                .allow_empty(true)
                .interact_text()?;
            app.stop_pulse();
            if answer.trim().eq_ignore_ascii_case("y") {
                model::record_mapping(&mut app.manifest, label, gpio, true)?;
                app.save()?;
                println!("Verified {label} as /gpio/{gpio}.");
            }
        }
        PulseStatus::Timeout | PulseStatus::LostConnection { .. } => {
            app.stop_pulse();
            println!("/gpio/{gpio} did not respond cleanly during verification.");
            app.wait_for_manual_reset()?;
        }
    }
    Ok(())
}
