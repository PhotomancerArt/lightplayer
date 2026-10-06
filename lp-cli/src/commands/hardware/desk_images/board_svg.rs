//! A board drawing as a standalone SVG file: Studio's own `BoardDiagram`
//! component, rendered server-side, with Studio's own diagram palette inlined.
//!
//! Nothing here draws. The geometry is `lpa-boards`' row engine and the
//! colours are the `lpb-*` block and `:root` tokens of Studio's stylesheet,
//! read out of that file at build time between its marker comments — so the
//! file can never drift from what Studio shows. A second renderer, or a
//! copied palette, is exactly the drift this avoids.

use anyhow::{Context, Result, bail};
use dioxus::prelude::*;
use lpa_boards::{BoardDiagram, board_by_id};

const STUDIO_CSS: &str = include_str!("../../../../../lp-app/lpa-studio-web/src/style.css");
const DIAGRAM_START: &str = "/* ---- board diagrams (lpa-boards BoardDiagram)";
const DIAGRAM_END: &str = "/* ---- end of board diagrams";

/// The drawing of LightPlayer board `board_id` (`seeed/xiao-esp32-c6`), pin
/// labels on, on a dark backdrop so it reads on any page.
pub fn board_svg(board_id: &str) -> Result<String> {
    let board = board_by_id(board_id)
        .with_context(|| format!("LightPlayer has no board `{board_id}`"))?
        .clone();
    let rendered = dioxus_ssr::render_element(rsx! {
        BoardDiagram { board, labels: true }
    });
    standalone(&rendered, &diagram_css()?)
}

/// The tokens the diagram block uses, then the block itself, scoped to the
/// drawing.
fn diagram_css() -> Result<String> {
    let tokens =
        between(STUDIO_CSS, "\n:root {", "\n}").context("style.css lost its :root block")?;
    let start = STUDIO_CSS
        .find(DIAGRAM_START)
        .context("style.css lost its board-diagram header")?;
    let end = STUDIO_CSS[start..]
        .find(DIAGRAM_END)
        .context("style.css lost its end-of-board-diagrams marker")?;
    let block = &STUDIO_CSS[start..start + end];
    Ok(format!("svg.lpb-diagram {{{tokens}\n}}\n{block}"))
}

/// Make the rendered `<svg>` a file a browser opens on its own: the SVG
/// namespace, the styles, and a backdrop behind the board.
fn standalone(rendered: &str, css: &str) -> Result<String> {
    let rendered = rendered.trim();
    let Some(open_end) = rendered.find('>') else {
        bail!("the diagram did not render to an element");
    };
    if !rendered.starts_with("<svg") {
        bail!("the diagram rendered to something other than <svg>");
    }
    let open = &rendered[..open_end];
    let view_box = attribute(open, "viewBox").context("the diagram has no viewBox")?;
    let open = if open.contains("xmlns=") {
        open.to_owned()
    } else {
        format!("{open} xmlns=\"http://www.w3.org/2000/svg\"")
    };
    let [x, y, w, h] = view_box_numbers(&view_box)?;
    let body = &rendered[open_end + 1..];
    Ok(format!(
        "{open}><style><![CDATA[{}]]></style><rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" \
         rx=\"8\" fill=\"var(--studio-color-bg)\"/>{body}\n",
        css.replace("]]>", "]] >")
    ))
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let start = tag.find(&format!("{name}=\""))? + name.len() + 2;
    let len = tag[start..].find('"')?;
    Some(tag[start..start + len].to_owned())
}

fn view_box_numbers(view_box: &str) -> Result<[f32; 4]> {
    let numbers: Vec<f32> = view_box
        .split([' ', ','])
        .filter(|part| !part.is_empty())
        .map(str::parse)
        .collect::<Result<_, _>>()
        .with_context(|| format!("viewBox `{view_box}` is not four numbers"))?;
    numbers
        .try_into()
        .map_err(|_| anyhow::anyhow!("viewBox `{view_box}` is not four numbers"))
}

fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = text.find(open)? + open.len();
    let len = text[start..].find(close)?;
    Some(&text[start..start + len])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_board_exports_a_well_formed_standalone_svg() {
        for board in lpa_boards::all_boards() {
            let svg = board_svg(&board.board_id)
                .unwrap_or_else(|err| panic!("{}: {err:#}", board.board_id));
            assert_well_formed(&svg, &board.board_id);
            assert!(svg.starts_with("<svg"), "{}", board.board_id);
            assert!(svg.contains("xmlns=\"http://www.w3.org/2000/svg\""));
            assert!(svg.contains(".lpb-pcb"), "the palette is inlined");
            assert!(svg.contains("--studio-color-bg:"), "the tokens are inlined");
            assert!(
                !svg.contains(".lpb-cat-"),
                "the catalog page's rules stay out"
            );
        }
    }

    #[test]
    fn the_xiao_c6_drawing_carries_its_pin_labels() {
        let svg = board_svg("seeed/xiao-esp32-c6").unwrap();
        assert!(svg.contains(">D10<"), "pin labels are drawn");
    }

    #[test]
    fn an_unknown_board_is_named() {
        let err = board_svg("acme/nope").unwrap_err().to_string();
        assert!(err.contains("acme/nope"), "{err}");
    }

    fn assert_well_formed(svg: &str, what: &str) {
        let mut reader = quick_xml::Reader::from_str(svg);
        let mut depth = 0i32;
        loop {
            match reader.read_event() {
                Ok(quick_xml::events::Event::Start(_)) => depth += 1,
                Ok(quick_xml::events::Event::End(_)) => depth -= 1,
                Ok(quick_xml::events::Event::Eof) => break,
                Ok(_) => {}
                Err(err) => panic!("{what}: not well-formed XML: {err}"),
            }
        }
        assert_eq!(depth, 0, "{what}: unbalanced tags");
    }
}
