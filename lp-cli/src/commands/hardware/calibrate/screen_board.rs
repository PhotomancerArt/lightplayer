use anyhow::Result;
use dialoguer::{Input, Select};

use super::app::{App, Route};
use super::model::{self, LabelStatus};
use super::ui::{BOLD, CYAN, DIM, GREEN, RED, YELLOW, paint, section, shortcut};

pub fn show(app: &mut App) -> Result<Route> {
    print_board(app);
    let default = model::next_unassigned_label(&app.manifest);
    let prompt = match &default {
        Some(label) => format!(
            "Enter=calibrate {label}, {}, {}, {}",
            shortcut('l', "ist"),
            shortcut('p', "ick label"),
            shortcut('q', "uit")
        ),
        None => format!(
            "Enter=edit label list, {}, {}, {}",
            shortcut('l', "ist"),
            shortcut('p', "ick label"),
            shortcut('q', "uit")
        ),
    };
    let command: String = Input::new()
        .with_prompt(prompt)
        .allow_empty(true)
        .interact_text()?;
    let command = command.trim();
    if command.is_empty() {
        return Ok(default.map(Route::Search).unwrap_or(Route::LabelList));
    }
    match command.to_ascii_lowercase().as_str() {
        "l" => Ok(Route::LabelList),
        "p" => pick_label(app),
        "v" => pick_status(app, LabelStatus::Assigned)
            .map(|label| label.map(Route::Pin).unwrap_or(Route::Board)),
        "r" => pick_status(app, LabelStatus::NotFound)
            .map(|label| label.map(Route::Search).unwrap_or(Route::Board)),
        "q" => Ok(Route::Quit),
        label => {
            model::ensure_label(&mut app.manifest, label)?;
            app.save()?;
            Ok(Route::Pin(label.to_string()))
        }
    }
}

fn print_board(app: &App) {
    println!();
    println!(
        "{} {}",
        paint(BOLD, "Board:"),
        paint(
            CYAN,
            &format!(
                "{}  {} {}",
                app.manifest.id, app.manifest.vendor, app.manifest.product
            )
        )
    );
    println!("{} {}", paint(BOLD, "Target:"), app.target);

    let rows = model::rows(&app.manifest);
    section("Labels");
    if rows.is_empty() {
        println!("  {}", paint(DIM, "none defined yet"));
    } else {
        for row in rows {
            let status = match row.status {
                LabelStatus::Assigned | LabelStatus::Verified => paint(GREEN, row.status.as_str()),
                LabelStatus::NotFound | LabelStatus::Skipped => paint(YELLOW, row.status.as_str()),
                LabelStatus::Unassigned => paint(DIM, row.status.as_str()),
            };
            let gpio = row
                .gpio
                .map(|gpio| format!("/gpio/{gpio}"))
                .unwrap_or_else(|| "-".into());
            println!("  {:<8} {:<12} {}", row.label, status, gpio);
        }
    }

    let reserved: Vec<_> = app
        .manifest
        .gpio
        .iter()
        .filter_map(|resource| {
            resource.reserved_reason.as_ref().map(|reason| {
                format!(
                    "{}  {}  ({reason})",
                    resource.address, resource.display_label
                )
            })
        })
        .collect();
    if !reserved.is_empty() {
        section("Dangerous / reserved");
        for item in reserved {
            println!("  {}", paint(RED, &item));
        }
    }
    println!();
    println!(
        "{}",
        paint(
            DIM,
            &format!(
                "Commands: {}, {}.",
                shortcut('v', "erify assigned"),
                shortcut('r', "etry not-found")
            )
        )
    );
}

fn pick_label(app: &App) -> Result<Route> {
    let rows = model::rows(&app.manifest);
    if rows.is_empty() {
        return Ok(Route::LabelList);
    }
    let items: Vec<_> = rows
        .iter()
        .map(|row| {
            let gpio = row
                .gpio
                .map(|gpio| format!("/gpio/{gpio}"))
                .unwrap_or_else(|| "-".into());
            format!("{}  {}  {gpio}", row.label, row.status.as_str())
        })
        .collect();
    let choice = Select::new()
        .with_prompt("Board label")
        .items(&items)
        .default(0)
        .interact()?;
    Ok(Route::Pin(rows[choice].label.clone()))
}

fn pick_status(app: &App, status: LabelStatus) -> Result<Option<String>> {
    let rows: Vec<_> = model::rows(&app.manifest)
        .into_iter()
        .filter(|row| row.status == status)
        .collect();
    if rows.is_empty() {
        println!("No labels with status {}.", status.as_str());
        return Ok(None);
    }
    let items: Vec<_> = rows.iter().map(|row| row.label.clone()).collect();
    let choice = Select::new()
        .with_prompt(format!("Choose {} label", status.as_str()))
        .items(&items)
        .default(0)
        .interact()?;
    Ok(Some(rows[choice].label.clone()))
}
