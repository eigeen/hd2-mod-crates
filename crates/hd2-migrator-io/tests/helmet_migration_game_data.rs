//! Real-game smoke test for the async/WASM helmet migration path.
//!
//! Set `HD2_TEST_DATA_DIR` to a Helldivers 2 `data/` directory to run it.

use hd2_migrator_io::archive::{StreamToc, TocEntry};
use hd2_migrator_io::constants::{MATERIAL_ID, TEX_ID, UNIT_ID};
use hd2_migrator_io::io::NativeDataSource;
use hd2_migrator_io::migrator::mode_a_web;
use hd2_migrator_io::unit::helmet_authority::HelmetMappingTable;
use hd2_migrator_io::web::{PatchBytes, WebMigrateOptions, list_target_options};
use std::path::PathBuf;

const SOURCE_HASH: &str = "1a2fc86abd27bf5b";
const SOURCE_NAME: &str = "A-35 Recon";
const TARGET_HASH: &str = "a856edff49cfdd95";
const TARGET_NAME: &str = "A-9 Helljumper";

#[test]
fn migrates_synthetic_helmet_patch_against_real_game_data() {
    let Some(data_dir) = std::env::var_os("HD2_TEST_DATA_DIR").map(PathBuf::from) else {
        eprintln!("[skip] HD2_TEST_DATA_DIR not set");
        return;
    };
    let mapping = HelmetMappingTable::bundled().expect("helmet mapping");
    let source_unit_id = mapping.unit_id(SOURCE_NAME).expect("source Unit");
    let target_unit_id = mapping.unit_id(TARGET_NAME).expect("target Unit");
    let patch_bytes = synthetic_patch(source_unit_id);
    let options = WebMigrateOptions {
        source_hash: Some(SOURCE_HASH.to_string()),
        target_hashes: vec![TARGET_HASH.to_string()],
        patch_suffix: None,
        no_padding: false,
        experimental_partial_remap: false,
    };

    let source = NativeDataSource::new(&data_dir);
    let results = pollster::block_on(mode_a_web::run(
        &patch_bytes,
        &options,
        &source,
        "Helmet",
        None,
    ))
    .expect("helmet migration against game data");

    let result = results.first().expect("one target result");
    assert_eq!(result.target_name, TARGET_NAME);
    assert!(has_unit(&result.patch, target_unit_id));
    assert!(!has_unit(&result.patch, source_unit_id));
    assert!(has_entry(&result.patch, 30, MATERIAL_ID));
    assert!(has_entry(&result.patch, 40, TEX_ID));
    assert_eq!(result.report.file_id_remapped, 1);
}

#[test]
fn resolves_every_helmet_name_to_a_current_game_archive() {
    let Some(data_dir) = std::env::var_os("HD2_TEST_DATA_DIR").map(PathBuf::from) else {
        eprintln!("[skip] HD2_TEST_DATA_DIR not set");
        return;
    };
    let mapping = HelmetMappingTable::bundled().expect("helmet mapping");
    let helmets = list_target_options("Helmet").expect("Helmet options");
    let source_unit_id = mapping.unit_id(SOURCE_NAME).expect("source Unit");
    let patch_bytes = synthetic_patch(source_unit_id);
    let options = WebMigrateOptions {
        source_hash: Some(SOURCE_HASH.to_string()),
        target_hashes: helmets.iter().map(|helmet| helmet.hash.clone()).collect(),
        patch_suffix: None,
        no_padding: true,
        experimental_partial_remap: false,
    };

    let source = NativeDataSource::new(&data_dir);
    let results = pollster::block_on(mode_a_web::run(
        &patch_bytes,
        &options,
        &source,
        "Helmet",
        None,
    ))
    .expect("resolve every helmet archive candidate");

    assert_eq!(results.len(), 106);
    let field_chemist = results
        .iter()
        .find(|result| result.target_name == "AF-91 Field Chemist")
        .expect("AF-91 result");
    assert_ne!(field_chemist.target_hash, "21a24072a79aeba8");
}

fn synthetic_patch(source_unit_id: u64) -> PatchBytes {
    let mut patch = StreamToc {
        name: "synthetic.patch_0".to_string(),
        entries: vec![
            TocEntry::new(source_unit_id, UNIT_ID),
            TocEntry::new(30, MATERIAL_ID),
            TocEntry::new(40, TEX_ID),
        ],
        ..Default::default()
    };
    let (toc, gpu, stream) = patch.serialize();
    PatchBytes {
        name: patch.name,
        toc,
        gpu,
        stream,
    }
}

fn has_unit(patch: &StreamToc, file_id: u64) -> bool {
    has_entry(patch, file_id, UNIT_ID)
}

fn has_entry(patch: &StreamToc, file_id: u64, type_id: u64) -> bool {
    patch
        .entries
        .iter()
        .any(|entry| entry.type_id == type_id && entry.file_id == file_id)
}
