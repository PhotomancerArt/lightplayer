use anyhow::{Result, bail};
use lpc_hardware::{HardwareBoardLabelFile, HardwareBoardLabelStatus, HardwareManifestFile};

use super::calibration_manifest_update::{
    apply_mapping, is_provisional_gpio_label, parse_gpio_address,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelRow {
    pub label: String,
    pub status: LabelStatus,
    pub gpio: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelStatus {
    Unassigned,
    Assigned,
    Verified,
    NotFound,
    Skipped,
}

impl LabelStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unassigned => "unassigned",
            Self::Assigned => "assigned",
            Self::Verified => "verified",
            Self::NotFound => "not found",
            Self::Skipped => "skipped",
        }
    }

    pub fn is_unassigned(self) -> bool {
        matches!(self, Self::Unassigned)
    }
}

pub fn sync_board_labels_from_gpio(manifest: &mut HardwareManifestFile) {
    let mut discovered = Vec::new();
    for resource in &manifest.gpio {
        let Some(gpio) = parse_gpio_address(&resource.address) else {
            continue;
        };
        if resource.reserved_reason.is_some()
            || is_provisional_gpio_label(gpio, &resource.display_label)
        {
            continue;
        }
        discovered.push((resource.display_label.clone(), resource.address.clone()));
    }
    discovered.sort();

    for (label, address) in discovered {
        let entry = ensure_label_entry(manifest, &label);
        if entry.gpio.is_none() {
            entry.gpio = Some(address);
        }
        if entry.status.is_none() {
            entry.status = Some(HardwareBoardLabelStatus::Assigned);
        }
    }
}

pub fn rows(manifest: &HardwareManifestFile) -> Vec<LabelRow> {
    let mut rows: Vec<_> = manifest
        .board_label
        .iter()
        .map(|entry| row(manifest, &entry.label))
        .collect();
    rows.sort_by(|left, right| {
        natural_label_key(&left.label).cmp(&natural_label_key(&right.label))
    });
    rows
}

pub fn row(manifest: &HardwareManifestFile, label: &str) -> LabelRow {
    let entry = manifest
        .board_label
        .iter()
        .find(|entry| entry.label.eq_ignore_ascii_case(label));
    let gpio = entry
        .and_then(|entry| entry.gpio.as_deref())
        .and_then(parse_gpio_address)
        .or_else(|| mapped_gpio_for_label(manifest, label));
    let status = match entry.and_then(|entry| entry.status) {
        Some(HardwareBoardLabelStatus::Verified) => LabelStatus::Verified,
        Some(HardwareBoardLabelStatus::NotFound) => LabelStatus::NotFound,
        Some(HardwareBoardLabelStatus::Skipped) => LabelStatus::Skipped,
        Some(HardwareBoardLabelStatus::Unassigned) | None if gpio.is_none() => {
            LabelStatus::Unassigned
        }
        _ if gpio.is_some() => LabelStatus::Assigned,
        _ => LabelStatus::Unassigned,
    };
    LabelRow {
        label: label.trim().to_string(),
        status,
        gpio,
    }
}

pub fn next_unassigned_label(manifest: &HardwareManifestFile) -> Option<String> {
    rows(manifest)
        .into_iter()
        .find(|row| row.status.is_unassigned())
        .map(|row| row.label)
}

pub fn ensure_label(manifest: &mut HardwareManifestFile, label: &str) -> Result<()> {
    validate_label(label)?;
    ensure_label_entry(manifest, label);
    Ok(())
}

pub fn replace_label_list(manifest: &mut HardwareManifestFile, labels: Vec<String>) -> Result<()> {
    if labels.is_empty() {
        bail!("board label list must not be empty");
    }
    let mut next = Vec::new();
    for label in labels {
        validate_label(&label)?;
        if next
            .iter()
            .any(|entry: &HardwareBoardLabelFile| entry.label.eq_ignore_ascii_case(label.trim()))
        {
            continue;
        }
        let existing = manifest
            .board_label
            .iter()
            .find(|entry| entry.label.eq_ignore_ascii_case(label.trim()))
            .cloned()
            .unwrap_or_else(|| HardwareBoardLabelFile::new(label.trim()));
        next.push(existing);
    }
    manifest.board_label = next;
    Ok(())
}

pub fn parse_label_list(text: &str) -> Result<Vec<String>> {
    let mut labels = Vec::new();
    for token in text
        .split(|ch: char| ch.is_ascii_whitespace() || ch == ',')
        .filter(|token| !token.trim().is_empty())
    {
        expand_label_token(token.trim(), &mut labels)?;
    }
    Ok(labels)
}

pub fn record_mapping(
    manifest: &mut HardwareManifestFile,
    label: &str,
    gpio: u32,
    verified: bool,
) -> Result<()> {
    validate_label(label)?;
    clear_label_from_gpio_resources(manifest, label, Some(gpio));
    apply_mapping(manifest, gpio, label)?;
    let entry = ensure_label_entry(manifest, label);
    entry.gpio = Some(format!("/gpio/{gpio}"));
    entry.status = Some(if verified {
        HardwareBoardLabelStatus::Verified
    } else {
        HardwareBoardLabelStatus::Assigned
    });
    entry.note = None;
    Ok(())
}

pub fn mark_not_found(manifest: &mut HardwareManifestFile, label: &str) -> Result<()> {
    validate_label(label)?;
    let entry = ensure_label_entry(manifest, label);
    entry.status = Some(HardwareBoardLabelStatus::NotFound);
    entry.note = None;
    Ok(())
}

pub fn mark_skipped(manifest: &mut HardwareManifestFile, label: &str) -> Result<()> {
    validate_label(label)?;
    let entry = ensure_label_entry(manifest, label);
    entry.status = Some(HardwareBoardLabelStatus::Skipped);
    Ok(())
}

pub fn unassign(manifest: &mut HardwareManifestFile, label: &str) -> Result<()> {
    validate_label(label)?;
    clear_label_from_gpio_resources(manifest, label, None);
    let entry = ensure_label_entry(manifest, label);
    entry.gpio = None;
    entry.status = Some(HardwareBoardLabelStatus::Unassigned);
    Ok(())
}

fn ensure_label_entry<'a>(
    manifest: &'a mut HardwareManifestFile,
    label: &str,
) -> &'a mut HardwareBoardLabelFile {
    let label = label.trim();
    if let Some(index) = manifest
        .board_label
        .iter()
        .position(|entry| entry.label.eq_ignore_ascii_case(label))
    {
        return &mut manifest.board_label[index];
    }
    manifest
        .board_label
        .push(HardwareBoardLabelFile::new(label));
    manifest
        .board_label
        .last_mut()
        .expect("inserted board label")
}

fn mapped_gpio_for_label(manifest: &HardwareManifestFile, label: &str) -> Option<u32> {
    manifest.gpio.iter().find_map(|resource| {
        if resource.display_label.eq_ignore_ascii_case(label) {
            parse_gpio_address(&resource.address)
        } else {
            None
        }
    })
}

fn clear_label_from_gpio_resources(
    manifest: &mut HardwareManifestFile,
    label: &str,
    except_gpio: Option<u32>,
) {
    for resource in &mut manifest.gpio {
        let Some(gpio) = parse_gpio_address(&resource.address) else {
            continue;
        };
        if except_gpio == Some(gpio) || !resource.display_label.eq_ignore_ascii_case(label) {
            continue;
        }
        ensure_resource_alias(resource, label);
        resource.display_label = format!("GPIO{gpio}");
        ensure_resource_alias(resource, &format!("IO{gpio}"));
    }
}

fn ensure_resource_alias(
    resource: &mut lpc_hardware::manifest::hardware_manifest_file::HardwareResourceFile,
    alias: &str,
) {
    if !resource.aliases.iter().any(|existing| existing == alias) {
        resource.aliases.push(alias.into());
    }
}

fn validate_label(label: &str) -> Result<()> {
    if label.trim().is_empty() {
        bail!("board label must not be empty");
    }
    Ok(())
}

fn expand_label_token(token: &str, labels: &mut Vec<String>) -> Result<()> {
    if let Some(expanded) = expand_bracket_range(token) {
        labels.extend(expanded);
        return Ok(());
    }
    if let Some((start, end)) = token.split_once('-') {
        if let Some(expanded) = expand_range(start.trim(), end.trim()) {
            labels.extend(expanded);
            return Ok(());
        }
    }
    validate_label(token)?;
    labels.push(token.to_string());
    Ok(())
}

fn expand_bracket_range(token: &str) -> Option<Vec<String>> {
    let open = token.find('[')?;
    let close = token[open + 1..].find(']')? + open + 1;
    if close + 1 != token.len() {
        return None;
    }
    let prefix = &token[..open];
    let range = &token[open + 1..close];
    let (start, end) = range.split_once('-')?;
    if prefix.is_empty() {
        return None;
    }
    let start: u32 = start.parse().ok()?;
    let end: u32 = end.parse().ok()?;
    if start > end {
        return None;
    }
    Some(
        (start..=end)
            .map(|number| format!("{prefix}{number}"))
            .collect(),
    )
}

fn expand_range(start: &str, end: &str) -> Option<Vec<String>> {
    let (start_prefix, start_number) = split_label_number(start)?;
    let (end_prefix, end_number) = split_label_number(end)?;
    if start_prefix != end_prefix || start_number > end_number {
        return None;
    }
    Some(
        (start_number..=end_number)
            .map(|number| format!("{start_prefix}{number}"))
            .collect(),
    )
}

fn split_label_number(label: &str) -> Option<(&str, u32)> {
    let first_digit = label.find(|ch: char| ch.is_ascii_digit())?;
    let (prefix, number) = label.split_at(first_digit);
    if prefix.is_empty() || number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((prefix, number.parse().ok()?))
}

fn natural_label_key(label: &str) -> (String, u32, String) {
    if let Some((prefix, number)) = split_label_number(label) {
        return (prefix.to_ascii_lowercase(), number, String::new());
    }
    (label.to_ascii_lowercase(), u32::MAX, label.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_hardware::HardwareTarget;

    #[test]
    fn expands_label_ranges() {
        assert_eq!(
            parse_label_list("D0-D3 SDA SCL").unwrap(),
            ["D0", "D1", "D2", "D3", "SDA", "SCL"]
        );
    }

    #[test]
    fn expands_bracket_label_ranges() {
        assert_eq!(
            parse_label_list("D[0-3] A[0-1]").unwrap(),
            ["D0", "D1", "D2", "D3", "A0", "A1"]
        );
    }

    #[test]
    fn records_label_mapping_status() {
        let mut manifest =
            HardwareManifestFile::new("seeed/xiao", HardwareTarget::Esp32c6, "seeed", "xiao");

        record_mapping(&mut manifest, "D3", 21, false).unwrap();

        let row = row(&manifest, "D3");
        assert_eq!(row.status, LabelStatus::Assigned);
        assert_eq!(row.gpio, Some(21));
    }

    #[test]
    fn next_unassigned_label_ignores_not_found_and_skipped() {
        let mut manifest =
            HardwareManifestFile::new("seeed/xiao", HardwareTarget::Esp32c6, "seeed", "xiao");
        replace_label_list(
            &mut manifest,
            vec!["D0".into(), "D1".into(), "D2".into(), "D3".into()],
        )
        .unwrap();
        record_mapping(&mut manifest, "D0", 0, false).unwrap();
        mark_not_found(&mut manifest, "D1").unwrap();
        mark_skipped(&mut manifest, "D2").unwrap();

        assert_eq!(next_unassigned_label(&manifest), Some("D3".into()));
    }

    #[test]
    fn remapping_label_frees_previous_gpio() {
        let mut manifest =
            HardwareManifestFile::new("seeed/xiao", HardwareTarget::Esp32c6, "seeed", "xiao");
        record_mapping(&mut manifest, "D3", 21, false).unwrap();

        record_mapping(&mut manifest, "D3", 3, false).unwrap();

        assert_eq!(row(&manifest, "D3").gpio, Some(3));
        let old = manifest
            .gpio
            .iter()
            .find(|resource| resource.address == "/gpio/21")
            .unwrap();
        assert_eq!(old.display_label, "GPIO21");
        assert!(old.aliases.iter().any(|alias| alias == "D3"));
    }
}
