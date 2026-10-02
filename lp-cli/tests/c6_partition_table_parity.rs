//! The C6 emulator's partition table is the product's.
//!
//! A direct-load emulator run stages a partition table at `0x8000`
//! (`lp_emu_esp32c6::flash::c6_partition_table_bytes`), because the C6
//! firmware reads `lpfs` from the flashed table at boot. The emulator crate
//! sits behind the MIT fence and may not read `lp-fw/fw-esp32c6/partitions.csv`,
//! so its table is a board model's facts — and this test, on the product side
//! of the fence, holds them equal to the real file compiled by espflash's own
//! encoder (`esp-idf-part`, exactly what `espflash save-image` writes).

use std::path::Path;

#[test]
fn the_emulators_c6_table_is_partitions_csv_compiled_by_espflash() {
    let csv_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../lp-fw/fw-esp32c6/partitions.csv");
    let csv = std::fs::read_to_string(&csv_path).expect("read fw-esp32c6/partitions.csv");
    let table = esp_idf_part::PartitionTable::try_from_str(&csv).expect("parse partitions.csv");
    let compiled = table.to_bin().expect("encode partitions.csv");

    let emulated = lp_emu_esp32c6::flash::c6_partition_table_bytes();
    assert_eq!(
        compiled.len(),
        emulated.len(),
        "a compiled table is {} bytes, the emulator's is {}",
        compiled.len(),
        emulated.len()
    );
    assert!(
        compiled[..] == emulated[..],
        "lp-emu-esp32c6's synthesized partition table no longer matches \
         lp-fw/fw-esp32c6/partitions.csv — move the constants in \
         lp-emu/esp/lp-emu-esp32c6/src/flash.rs with the CSV"
    );
}

#[test]
fn the_emulators_lpfs_facts_are_the_csvs_lpfs_row() {
    let csv_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../lp-fw/fw-esp32c6/partitions.csv");
    let csv = std::fs::read_to_string(&csv_path).expect("read fw-esp32c6/partitions.csv");
    let table = esp_idf_part::PartitionTable::try_from_str(&csv).expect("parse partitions.csv");
    let lpfs = table.find("lpfs").expect("an lpfs row");
    let factory = table.find("factory").expect("a factory row");
    assert_eq!(lpfs.offset(), lp_emu_esp32c6::flash::LPFS_OFFSET);
    assert_eq!(lpfs.size(), lp_emu_esp32c6::flash::LPFS_LEN);
    assert_eq!(factory.offset(), lp_emu_esp32c6::flash::FACTORY_OFFSET);
    assert_eq!(factory.size(), lp_emu_esp32c6::flash::FACTORY_LEN);
}
