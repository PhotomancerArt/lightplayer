use anyhow::{Result, bail};
use lpc_shared::hardware::{
    HardwareCapability, HardwareManifestFile, hardware_manifest_file::HardwareResourceFile,
};

const DANGEROUS_REASON: &str = "crashed or timed out during calibration";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpioCandidate {
    pub gpio: u32,
    pub address: String,
    pub display_label: String,
}

pub fn gpio_candidates(manifest: &HardwareManifestFile) -> Vec<GpioCandidate> {
    let mut candidates: Vec<_> = manifest
        .gpio
        .iter()
        .filter(|resource| resource.reserved_reason.is_none())
        .filter_map(|resource| {
            parse_gpio_address(&resource.address).and_then(|gpio| {
                is_provisional_gpio_label(gpio, &resource.display_label).then(|| GpioCandidate {
                    gpio,
                    address: resource.address.clone(),
                    display_label: resource.display_label.clone(),
                })
            })
        })
        .collect();
    candidates.sort_by_key(|candidate| candidate.gpio);
    candidates
}

pub fn apply_mapping(
    manifest: &mut HardwareManifestFile,
    gpio: u32,
    board_label: &str,
) -> Result<()> {
    let resource = find_or_insert_gpio(manifest, gpio, board_label);
    let previous_label = resource.display_label.trim().to_string();
    if !previous_label.is_empty()
        && previous_label != board_label
        && !resource
            .aliases
            .iter()
            .any(|alias| alias == &previous_label)
    {
        resource.aliases.push(previous_label);
    }
    resource.display_label = board_label.trim().to_string();
    ensure_alias(resource, &format!("GPIO{gpio}"));
    ensure_alias(resource, &format!("IO{gpio}"));
    resource.reserved_reason = None;
    Ok(())
}

pub fn apply_dangerous(manifest: &mut HardwareManifestFile, gpio: u32) -> Result<()> {
    let label = format!("GPIO{gpio}");
    let resource = find_or_insert_gpio(manifest, gpio, &label);
    resource.reserved_reason = Some(DANGEROUS_REASON.into());
    ensure_alias(resource, &format!("GPIO{gpio}"));
    ensure_alias(resource, &format!("IO{gpio}"));
    Ok(())
}

pub fn parse_gpio_address(address: &str) -> Option<u32> {
    address.strip_prefix("/gpio/")?.parse().ok()
}

pub fn is_provisional_gpio_label(gpio: u32, label: &str) -> bool {
    label == format!("GPIO{gpio}") || label == format!("IO{gpio}") || label == gpio.to_string()
}

fn find_or_insert_gpio<'a>(
    manifest: &'a mut HardwareManifestFile,
    gpio: u32,
    fallback_label: &str,
) -> &'a mut HardwareResourceFile {
    let address = format!("/gpio/{gpio}");
    if let Some(index) = manifest
        .gpio
        .iter()
        .position(|resource| resource.address == address)
    {
        return &mut manifest.gpio[index];
    }
    manifest.gpio.push(HardwareResourceFile {
        address,
        display_label: fallback_label.into(),
        capabilities: vec![
            HardwareCapability::GpioOutput,
            HardwareCapability::GpioInput,
        ],
        aliases: vec![format!("GPIO{gpio}"), format!("IO{gpio}")],
        location: None,
        reserved_reason: None,
    });
    manifest.gpio.last_mut().expect("inserted GPIO resource")
}

fn ensure_alias(resource: &mut HardwareResourceFile, alias: &str) {
    if resource.display_label == alias {
        return;
    }
    if !resource.aliases.iter().any(|existing| existing == alias) {
        resource.aliases.push(alias.into());
    }
}

pub fn validate_board_label(label: &str) -> Result<()> {
    if label.trim().is_empty() {
        bail!("board label must not be empty");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_shared::hardware::HardwareTarget;

    #[test]
    fn mapping_preserves_previous_display_label_as_alias() {
        let mut manifest =
            HardwareManifestFile::new("seeed/xiao", HardwareTarget::Esp32c6, "seeed", "xiao");
        manifest.gpio.push(HardwareResourceFile::new(
            "/gpio/18",
            "GPIO18",
            [
                HardwareCapability::GpioOutput,
                HardwareCapability::GpioInput,
            ],
        ));

        apply_mapping(&mut manifest, 18, "D6").unwrap();

        let resource = &manifest.gpio[0];
        assert_eq!(resource.display_label, "D6");
        assert!(resource.aliases.iter().any(|alias| alias == "GPIO18"));
        assert!(resource.aliases.iter().any(|alias| alias == "IO18"));
    }

    #[test]
    fn dangerous_pin_gets_reserved_reason() {
        let mut manifest =
            HardwareManifestFile::new("seeed/xiao", HardwareTarget::Esp32c6, "seeed", "xiao");

        apply_dangerous(&mut manifest, 12).unwrap();

        let resource = &manifest.gpio[0];
        assert_eq!(resource.address, "/gpio/12");
        assert_eq!(resource.reserved_reason.as_deref(), Some(DANGEROUS_REASON));
    }

    #[test]
    fn candidates_skip_mapped_and_reserved_pins() {
        let mut manifest =
            HardwareManifestFile::new("seeed/xiao", HardwareTarget::Esp32c6, "seeed", "xiao");
        manifest.gpio.push(HardwareResourceFile::new(
            "/gpio/0",
            "D0",
            [
                HardwareCapability::GpioOutput,
                HardwareCapability::GpioInput,
            ],
        ));
        manifest.gpio.push(HardwareResourceFile::new(
            "/gpio/1",
            "GPIO1",
            [
                HardwareCapability::GpioOutput,
                HardwareCapability::GpioInput,
            ],
        ));
        apply_dangerous(&mut manifest, 2).unwrap();

        let candidates = gpio_candidates(&manifest);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].gpio, 1);
    }
}
