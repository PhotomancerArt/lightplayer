//! What the journeys share: opening a project, an editor laid out as the
//! page lays it out, and reading where lamps land and what the inspector
//! says.

#![allow(dead_code, reason = "each journey uses only some of these")]

use lab_mapping_files::Opened;
use lab_mapping_files::embedded::Embedded;
use lab_mapping_model::*;

pub fn open(e: &Embedded) -> Opened {
    Opened::open(&e.files()).unwrap_or_else(|err| panic!("{}: {err}", e.key))
}

/// An editor on `p`, laid out the way the page does it.
pub fn editor_of(p: Project) -> Editor {
    let mut e = Editor::new(p);
    let placement = lab_layout::canvas_layout(&e.layout_input());
    e.set_placement(&placement);
    e.resize(Vec2::new(1200.0, 800.0));
    e
}

/// Where one lamp lands: (port key, pixel).
pub fn landing(p: &Project, object: &ObjectId, lamp: u32) -> Option<(String, u32)> {
    p.effective_patch()
        .landing(object, lamp)
        .map(|l| (l.port, l.pixel))
}

/// The port an object starts on and its first and last pixels, in lamp
/// order (a reversed object's first pixel is its highest).
pub fn span(p: &Project, object: &ObjectId) -> Option<(String, u32, u32)> {
    let n = p.object(object)?.lamps.len() as u32;
    let first = landing(p, object, 0)?;
    let last = landing(p, object, n - 1)?;
    Some((first.0, first.1, last.1))
}

/// Every patching row the inspector shows for the selection.
pub fn rows(e: &Editor) -> Vec<UiItem> {
    e.view_inspector()
        .sections
        .into_iter()
        .flat_map(|s| s.items)
        .collect()
}

/// A choice row's options and what is chosen (the last with that label:
/// the object's, after the component's).
pub fn choice(e: &Editor, label: &str) -> (Vec<String>, Option<usize>) {
    rows(e)
        .into_iter()
        .rev()
        .find_map(|i| match i {
            UiItem::PatchChoice {
                label: l,
                options,
                chosen,
                ..
            } if l == label => Some((options, chosen)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {label} choice"))
}

/// Choose the option of a choice row that starts with `words`.
pub fn choose(e: &mut Editor, field: PatchField, label: &str, words: &str) {
    let (options, _) = choice(e, label);
    let option = options
        .iter()
        .position(|o| o.starts_with(words))
        .unwrap_or_else(|| panic!("{label} has no {words}: {options:?}"));
    e.patch_input(PatchInput::Choose { field, option });
}

/// The hints the inspector shows.
pub fn hints(e: &Editor) -> Vec<String> {
    rows(e)
        .into_iter()
        .filter_map(|i| match i {
            UiItem::Hint(h) => Some(h),
            _ => None,
        })
        .collect()
}
