use anyhow::Result;
use dialoguer::Input;

use super::app::{App, Route};
use super::model;
use super::ui::{BOLD, DIM, paint, section};

pub fn show(app: &mut App) -> Result<Route> {
    loop {
        print_labels(app);
        let input: String = Input::new()
            .with_prompt("Labels/ranges, + add, d delete, b back")
            .allow_empty(true)
            .interact_text()?;
        let input = input.trim();
        if input.is_empty() || input.eq_ignore_ascii_case("b") {
            return Ok(Route::Board);
        }
        if let Some(rest) = input.strip_prefix('+') {
            let mut labels: Vec<_> = model::rows(&app.manifest)
                .into_iter()
                .map(|row| row.label)
                .collect();
            labels.extend(model::parse_label_list(rest)?);
            model::replace_label_list(&mut app.manifest, labels)?;
            app.save()?;
            continue;
        }
        if input.eq_ignore_ascii_case("d") {
            delete_label(app)?;
            continue;
        }
        model::replace_label_list(&mut app.manifest, model::parse_label_list(input)?)?;
        app.save()?;
    }
}

fn print_labels(app: &App) {
    section("Board Labels");
    let labels: Vec<_> = model::rows(&app.manifest)
        .into_iter()
        .map(|row| row.label)
        .collect();
    if labels.is_empty() {
        println!("  {}", paint(DIM, "none defined yet"));
        println!();
        println!(
            "{}",
            paint(
                DIM,
                "Example: D0-D10 SDA SCL TX RX. Enter replaces the list."
            )
        );
    } else {
        println!("  {}", labels.join(" "));
        println!();
        println!(
            "{}",
            paint(DIM, "+ D11 adds labels; entering a full list replaces it.")
        );
    }
}

fn delete_label(app: &mut App) -> Result<()> {
    let label: String = Input::new()
        .with_prompt("Delete label")
        .allow_empty(true)
        .interact_text()?;
    let label = label.trim();
    if label.is_empty() {
        return Ok(());
    }
    app.manifest
        .board_label
        .retain(|entry| !entry.label.eq_ignore_ascii_case(label));
    app.save()?;
    println!("{} {label}", paint(BOLD, "Deleted"));
    Ok(())
}
