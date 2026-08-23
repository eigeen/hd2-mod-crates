//! Read-only integration check against an installed game's `data` directory.

use hd2_migrator_io::archive::toc_only::TocOnlyPackage;
use hd2_migrator_io::constants::UNIT_ID;
use hd2_migrator_io::io::{BundleSlicer, DataSource, NativeDataSource};
use hd2_migrator_io::web::{MissingUnitPolicy, UnitRepatchOptions, repatch_units};
use std::path::{Path, PathBuf};

const VERSION_OFFSET: usize = 0x2c;
const LAYOUT_LIST_OFFSET_FIELD: usize = 0x5c;
const LEGACY_VERSION: u32 = 10_800_437;
const CURRENT_VERSION: u32 = 10_800_438;
const COMPONENT_CAPACITY: usize = 16;
const COMPONENTS_OFFSET: usize = 8;
const COMPONENT_SIZE: usize = 20;
const COMPONENT_FORMAT_OFFSET: usize = 4;
const COMPONENT_COUNT_OFFSET: usize = COMPONENTS_OFFSET + COMPONENT_CAPACITY * COMPONENT_SIZE;

#[test]
#[ignore = "requires HD2_TEST_DATA_DIR or a local Helldivers 2 install"]
fn synthetic_old_unit_is_restored_from_installed_game() {
    let data_dir = game_data_dir();
    let source = NativeDataSource::new(&data_dir);
    let (archive, package, latest_unit, legacy_toc) = find_first_upgradable_unit(&source);
    let mut patch = package;
    patch
        .entries
        .retain(|entry| entry.file_id == latest_unit.file_id && entry.type_id == UNIT_ID);
    patch.entries[0].toc_data = legacy_toc;
    let patch_toc = patch.serialize().expect("serialize synthetic old patch");
    let options = UnitRepatchOptions {
        missing_unit_policy: MissingUnitPolicy::Fail,
    };
    let result = pollster::block_on(repatch_units(
        &format!("{archive}.patch_0"),
        &patch_toc,
        options,
        &source,
    ))
    .expect("repatch against installed game");
    let output = TocOnlyPackage::parse(&result.toc).expect("parse output");
    let output_unit = output
        .entries
        .iter()
        .find(|entry| entry.type_id == UNIT_ID)
        .expect("output Unit");
    assert_eq!(output_unit.toc_data, latest_unit.toc_data);
    assert_eq!(result.summary.updated_units, 1);
    assert!(result.summary.converted_formats > 0);
    assert_eq!(result.summary.scanned_archives, 1);
}

fn find_first_upgradable_unit(
    source: &NativeDataSource,
) -> (
    String,
    TocOnlyPackage,
    hd2_migrator_io::archive::toc_only::TocOnlyEntry,
    Vec<u8>,
) {
    if source.base().join("bundles.nxa").is_file() {
        return find_first_bundled_upgradable_unit(source);
    }
    let packages = pollster::block_on(source.list_packages()).expect("list archives");
    for archive in packages {
        let bytes = std::fs::read(source.base().join(&archive)).expect("read archive TOC");
        let Ok(package) = TocOnlyPackage::parse(&bytes) else {
            continue;
        };
        if let Some((entry, legacy_toc)) = first_upgradable_entry(&package) {
            return (archive, package.clone(), entry, legacy_toc);
        }
    }
    panic!("no Unit resource found under {}", source.base().display());
}

fn find_first_bundled_upgradable_unit(
    source: &NativeDataSource,
) -> (
    String,
    TocOnlyPackage,
    hd2_migrator_io::archive::toc_only::TocOnlyEntry,
    Vec<u8>,
) {
    let slicer = pollster::block_on(BundleSlicer::open(source)).expect("open bundle index");
    let mut packages = slicer
        .packages
        .keys()
        .filter(|name| name.len() == 16 && name.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .cloned()
        .collect::<Vec<_>>();
    packages.sort();
    for archive in packages {
        let bytes =
            pollster::block_on(slicer.load_package(source, &archive)).expect("load bundled TOC");
        let Ok(package) = TocOnlyPackage::parse(&bytes) else {
            continue;
        };
        if let Some((entry, legacy_toc)) = first_upgradable_entry(&package) {
            return (archive, package.clone(), entry, legacy_toc);
        }
    }
    panic!(
        "no bundled Unit resource found under {}",
        source.base().display()
    );
}

fn first_upgradable_entry(
    package: &TocOnlyPackage,
) -> Option<(hd2_migrator_io::archive::toc_only::TocOnlyEntry, Vec<u8>)> {
    package.entries.iter().find_map(|entry| {
        if entry.type_id != UNIT_ID {
            return None;
        }
        downgrade_current_unit(&entry.toc_data).map(|legacy| (entry.clone(), legacy))
    })
}

/// Builds a structurally valid legacy fixture by reversing the verified format mapping.
fn downgrade_current_unit(current: &[u8]) -> Option<Vec<u8>> {
    if read_u32(current, VERSION_OFFSET)? != CURRENT_VERSION {
        return None;
    }
    let list_start = read_u32(current, LAYOUT_LIST_OFFSET_FIELD)? as usize;
    if list_start == 0 {
        return None;
    }
    let layout_count = read_u32(current, list_start)? as usize;
    let mut legacy = current.to_vec();
    let mut converted = 0;
    for layout_index in 0..layout_count {
        converted += downgrade_layout(&mut legacy, list_start, layout_index)?;
    }
    if converted == 0 {
        return None;
    }
    write_u32(&mut legacy, VERSION_OFFSET, LEGACY_VERSION)?;
    Some(legacy)
}

fn downgrade_layout(unit: &mut [u8], list_start: usize, layout_index: usize) -> Option<usize> {
    let offset_field = list_start.checked_add(4 + layout_index.checked_mul(4)?)?;
    let record_start = list_start.checked_add(read_u32(unit, offset_field)? as usize)?;
    let count = read_u64(unit, record_start.checked_add(COMPONENT_COUNT_OFFSET)?)? as usize;
    if count > COMPONENT_CAPACITY {
        return None;
    }
    let components_start = record_start.checked_add(COMPONENTS_OFFSET)?;
    let mut converted = 0;
    for component_index in 0..count {
        let format_offset = components_start
            .checked_add(component_index.checked_mul(COMPONENT_SIZE)?)?
            .checked_add(COMPONENT_FORMAT_OFFSET)?;
        let current_format = read_u32(unit, format_offset)?;
        let legacy_format = legacy_stream_format(current_format)?;
        converted += usize::from(current_format != legacy_format);
        write_u32(unit, format_offset, legacy_format)?;
    }
    Some(converted)
}

fn legacy_stream_format(format: u32) -> Option<u32> {
    match format {
        0..=4 => Some(format),
        24 => Some(20),
        28 => Some(24),
        29 => Some(25),
        30 => Some(26),
        33 => Some(29),
        35 => Some(31),
        _ => None,
    }
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn read_u64(data: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        data.get(offset..offset.checked_add(8)?)?.try_into().ok()?,
    ))
}

fn write_u32(data: &mut [u8], offset: usize, value: u32) -> Option<()> {
    data.get_mut(offset..offset.checked_add(4)?)?
        .copy_from_slice(&value.to_le_bytes());
    Some(())
}

fn game_data_dir() -> PathBuf {
    let configured = std::env::var_os("HD2_TEST_DATA_DIR")
        .map(PathBuf::from)
        .expect("set HD2_TEST_DATA_DIR to the game root or data directory");
    let nested = configured.join("data");
    if is_data_dir(&nested) {
        nested
    } else {
        configured
    }
}

fn is_data_dir(path: &Path) -> bool {
    path.is_dir()
}
