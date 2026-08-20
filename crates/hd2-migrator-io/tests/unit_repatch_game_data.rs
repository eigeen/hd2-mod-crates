//! Read-only integration check against an installed game's `data` directory.

use hd2_migrator_io::archive::toc_only::TocOnlyPackage;
use hd2_migrator_io::constants::UNIT_ID;
use hd2_migrator_io::io::{BundleSlicer, DataSource, NativeDataSource};
use hd2_migrator_io::web::{MissingUnitPolicy, UnitRepatchOptions, repatch_units};
use std::path::{Path, PathBuf};

#[test]
#[ignore = "requires HD2_TEST_DATA_DIR or a local Helldivers 2 install"]
fn synthetic_old_unit_is_restored_from_installed_game() {
    let data_dir = game_data_dir();
    let source = NativeDataSource::new(&data_dir);
    let (archive, package, latest_unit) = find_first_unit(&source);
    let mut patch = package;
    patch
        .entries
        .retain(|entry| entry.file_id == latest_unit.file_id && entry.type_id == UNIT_ID);
    patch.entries[0].toc_data[0x2c..0x30].copy_from_slice(&0x00a4_cd37u32.to_le_bytes());
    let patch_toc = patch.serialize().expect("serialize synthetic old patch");
    let options = UnitRepatchOptions {
        missing_unit_policy: MissingUnitPolicy::Fail,
        culling_policy: Default::default(),
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
    assert_eq!(result.summary.scanned_archives, 1);
}

fn find_first_unit(
    source: &NativeDataSource,
) -> (
    String,
    TocOnlyPackage,
    hd2_migrator_io::archive::toc_only::TocOnlyEntry,
) {
    if source.base().join("bundles.nxa").is_file() {
        return find_first_bundled_unit(source);
    }
    let packages = pollster::block_on(source.list_packages()).expect("list archives");
    for archive in packages {
        let bytes = std::fs::read(source.base().join(&archive)).expect("read archive TOC");
        let Ok(package) = TocOnlyPackage::parse(&bytes) else {
            continue;
        };
        if let Some(entry) = package
            .entries
            .iter()
            .find(|entry| entry.type_id == UNIT_ID)
        {
            return (archive, package.clone(), entry.clone());
        }
    }
    panic!("no Unit resource found under {}", source.base().display());
}

fn find_first_bundled_unit(
    source: &NativeDataSource,
) -> (
    String,
    TocOnlyPackage,
    hd2_migrator_io::archive::toc_only::TocOnlyEntry,
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
        if let Some(entry) = package
            .entries
            .iter()
            .find(|entry| entry.type_id == UNIT_ID)
        {
            return (archive, package.clone(), entry.clone());
        }
    }
    panic!(
        "no bundled Unit resource found under {}",
        source.base().display()
    );
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
