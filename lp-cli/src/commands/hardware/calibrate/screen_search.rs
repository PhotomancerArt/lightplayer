use anyhow::Result;
use dialoguer::{Confirm, Input};

use super::app::{App, PulseStatus, Route, is_serial_disconnect};
use super::calibration_command::PromptCommand;
use super::calibration_manifest_update::{apply_dangerous, gpio_candidates};
use super::model;
use super::ui::{BOLD, DIM, paint, section};

pub fn show(app: &mut App, label: String) -> Result<Route> {
    model::ensure_label(&mut app.manifest, &label)?;
    app.save()?;
    let mut index = 0;
    print_intro(&label);
    loop {
        let candidates = gpio_candidates(&app.manifest);
        if candidates.is_empty() {
            println!("No GPIO candidates remain.");
            model::mark_not_found(&mut app.manifest, &label)?;
            app.save()?;
            return Ok(Route::Board);
        }
        if index >= candidates.len() {
            match prompt_not_found(&label)? {
                NotFoundCommand::Mark => {
                    model::mark_not_found(&mut app.manifest, &label)?;
                    app.save()?;
                    return Ok(Route::Board);
                }
                NotFoundCommand::Retry => {
                    index = 0;
                    continue;
                }
                NotFoundCommand::Back => return Ok(Route::Board),
            }
        }
        let candidate = candidates[index].clone();
        let pulse_status = match app.pulse_candidate(&candidate) {
            Err(error) if is_serial_disconnect(&error) => PulseStatus::LostConnection {
                message: error.to_string(),
            },
            Err(error) => return Err(error),
            Ok(status) => status,
        };
        match pulse_status {
            PulseStatus::Alive => match prompt_square_wave(&label, &candidate.address)? {
                SearchCommand::Next => {
                    app.stop_pulse();
                    index += 1;
                }
                SearchCommand::Previous => {
                    app.stop_pulse();
                    index = index.saturating_sub(1);
                }
                SearchCommand::Yes => {
                    app.stop_pulse();
                    model::record_mapping(&mut app.manifest, &label, candidate.gpio, false)?;
                    app.save()?;
                    println!();
                    println!("Recorded {label} as {}.", candidate.address);
                    return Ok(Route::Board);
                }
                SearchCommand::Skip => {
                    app.stop_pulse();
                    model::mark_skipped(&mut app.manifest, &label)?;
                    app.save()?;
                    return Ok(Route::Board);
                }
                SearchCommand::Back => {
                    app.stop_pulse();
                    return Ok(Route::Board);
                }
                SearchCommand::Quit => {
                    app.stop_pulse();
                    return Ok(Route::Quit);
                }
            },
            PulseStatus::Timeout | PulseStatus::LostConnection { .. } => {
                if let PulseStatus::LostConnection { message } = &pulse_status {
                    println!(
                        "{} lost USB serial while being tested ({message}). The device may have reset or this pin may be dangerous.",
                        candidate.address
                    );
                } else {
                    println!(
                        "{} did not produce calibration logs. The device may have reset or this pin may be dangerous.",
                        candidate.address
                    );
                }
                if Confirm::new()
                    .with_prompt(format!(
                        "Mark {} as dangerous and skip it in future calibration?",
                        candidate.address
                    ))
                    .default(false)
                    .interact()?
                {
                    apply_dangerous(&mut app.manifest, candidate.gpio)?;
                    app.save()?;
                    index = index.min(gpio_candidates(&app.manifest).len().saturating_sub(1));
                } else {
                    index += 1;
                }
                app.wait_for_manual_reset()?;
            }
        }
    }
}

fn print_intro(label: &str) {
    section(&format!("Searching for {label}"));
    println!("Scope that pad, then press Enter to scan.");
    println!(
        "{}",
        paint(
            DIM,
            "Keys: Enter=no/next, y=found, p=previous, s=skip label, b=board, q=quit."
        )
    );
    println!();
}

fn prompt_square_wave(label: &str, address: &str) -> Result<SearchCommand> {
    loop {
        let answer: String = Input::new()
            .with_prompt(format!("Is {label} present on {address}? (q/b/s/p/y/N)"))
            .allow_empty(true)
            .interact_text()?;
        let answer = answer.trim();
        if answer.eq_ignore_ascii_case("b") {
            return Ok(SearchCommand::Back);
        }
        if answer.eq_ignore_ascii_case("s") {
            return Ok(SearchCommand::Skip);
        }
        match PromptCommand::parse(answer) {
            Ok(PromptCommand::Next) => return Ok(SearchCommand::Next),
            Ok(PromptCommand::Previous) => return Ok(SearchCommand::Previous),
            Ok(PromptCommand::Yes) => return Ok(SearchCommand::Yes),
            Ok(PromptCommand::Quit) => return Ok(SearchCommand::Quit),
            Err(message) => println!("{message}, b, or s"),
        }
    }
}

fn prompt_not_found(label: &str) -> Result<NotFoundCommand> {
    println!();
    println!("{}", paint(BOLD, &format!("{label} was not found.")));
    loop {
        let answer: String = Input::new()
            .with_prompt("Enter=mark not found, r=retry, b=board")
            .allow_empty(true)
            .interact_text()?;
        match answer.trim().to_ascii_lowercase().as_str() {
            "" => return Ok(NotFoundCommand::Mark),
            "r" => return Ok(NotFoundCommand::Retry),
            "b" => return Ok(NotFoundCommand::Back),
            _ => println!("Expected Enter, r, or b."),
        }
    }
}

enum SearchCommand {
    Next,
    Previous,
    Yes,
    Skip,
    Back,
    Quit,
}

enum NotFoundCommand {
    Mark,
    Retry,
    Back,
}
