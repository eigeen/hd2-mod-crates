//! Real-game regression tests for mixed-equipment Unit handling in the web path.

use hd2_migrator_io::archive::{StreamToc, TocEntry};
use hd2_migrator_io::constants::UNIT_ID;
use hd2_migrator_io::io::NativeDataSource;
use hd2_migrator_io::migrator::mode_a_web;
use hd2_migrator_io::unit::authority::ArmorMappingTable;
use hd2_migrator_io::unit::helmet_authority::HelmetMappingTable;
use hd2_migrator_io::web::{PatchBytes, UnmatchedUnitPolicy, WebMigrateOptions};
use std::path::PathBuf;

const I102_HASH: &str = "57cebd7e5f985d45";
const I102_NAME: &str = "I-102 Draconaught";
const I102_UNMAPPED_BODY: u64 = 2_673_048_133_047_250_239;
const TG8_ARMOR_HASH: &str = "1434ec7cf0edf1bc";
const A9_HELMET_HASH: &str = "a856edff49cfdd95";
const A9_HELMET_NAME: &str = "A-9 Helljumper";
const A35_HELMET_HASH: &str = "1a2fc86abd27bf5b";

#[test]
fn armor_migration_drops_or_keeps_unmatched_and_helmet_units() {
    let Some(data_dir) = game_data_dir() else {
        return;
    };
    let armor_mapping = ArmorMappingTable::bundled().expect("armor mapping");
    let helmet_mapping = HelmetMappingTable::bundled().expect("helmet mapping");
    let mut unit_ids = armor_mapping
        .armor(I102_NAME)
        .expect("I-102 mapping")
        .all_file_ids();
    let helmet_unit = helmet_mapping
        .unit_id(A9_HELMET_NAME)
        .expect("A-9 helmet Unit");
    unit_ids.extend([I102_UNMAPPED_BODY, helmet_unit]);
    let patch = synthetic_patch(&unit_ids);

    let dropped = migrate(
        &data_dir,
        &patch,
        MigrationCase {
            category: "Armor",
            source_hash: I102_HASH,
            target_hash: TG8_ARMOR_HASH,
            policy: UnmatchedUnitPolicy::Drop,
        },
    );
    assert!(!has_unit(&dropped.patch, I102_UNMAPPED_BODY));
    assert!(!has_unit(&dropped.patch, helmet_unit));
    assert_mixed_warning(&dropped, "Helmet");

    let kept = migrate(
        &data_dir,
        &patch,
        MigrationCase {
            category: "Armor",
            source_hash: I102_HASH,
            target_hash: TG8_ARMOR_HASH,
            policy: UnmatchedUnitPolicy::Keep,
        },
    );
    assert!(has_unit(&kept.patch, I102_UNMAPPED_BODY));
    assert!(has_unit(&kept.patch, helmet_unit));
    assert_mixed_warning(&kept, "Helmet");
}

#[test]
fn helmet_migration_drops_or_keeps_armor_units() {
    let Some(data_dir) = game_data_dir() else {
        return;
    };
    let armor_mapping = ArmorMappingTable::bundled().expect("armor mapping");
    let helmet_mapping = HelmetMappingTable::bundled().expect("helmet mapping");
    let helmet_unit = helmet_mapping
        .unit_id(A9_HELMET_NAME)
        .expect("A-9 helmet Unit");
    let armor_unit = armor_mapping
        .armor(I102_NAME)
        .expect("I-102 mapping")
        .all_file_ids()[0];
    let patch = synthetic_patch(&[helmet_unit, armor_unit, I102_UNMAPPED_BODY]);

    let dropped = migrate(
        &data_dir,
        &patch,
        MigrationCase {
            category: "Helmet",
            source_hash: A9_HELMET_HASH,
            target_hash: A35_HELMET_HASH,
            policy: UnmatchedUnitPolicy::Drop,
        },
    );
    assert!(!has_unit(&dropped.patch, armor_unit));
    assert!(!has_unit(&dropped.patch, I102_UNMAPPED_BODY));
    assert_mixed_warning(&dropped, "Armor");

    let kept = migrate(
        &data_dir,
        &patch,
        MigrationCase {
            category: "Helmet",
            source_hash: A9_HELMET_HASH,
            target_hash: A35_HELMET_HASH,
            policy: UnmatchedUnitPolicy::Keep,
        },
    );
    assert!(has_unit(&kept.patch, armor_unit));
    assert!(has_unit(&kept.patch, I102_UNMAPPED_BODY));
    assert_mixed_warning(&kept, "Armor");
}

fn game_data_dir() -> Option<PathBuf> {
    let data_dir = std::env::var_os("HD2_TEST_DATA_DIR").map(PathBuf::from);
    if data_dir.is_none() {
        eprintln!("[skip] HD2_TEST_DATA_DIR not set");
    }
    data_dir
}

struct MigrationCase<'a> {
    category: &'a str,
    source_hash: &'a str,
    target_hash: &'a str,
    policy: UnmatchedUnitPolicy,
}

fn migrate(
    data_dir: &PathBuf,
    patch: &PatchBytes,
    case: MigrationCase<'_>,
) -> mode_a_web::WebTargetResult {
    let options = WebMigrateOptions {
        source_hash: Some(case.source_hash.to_string()),
        target_hashes: vec![case.target_hash.to_string()],
        patch_suffix: None,
        no_padding: true,
        unmatched_unit_policy: case.policy,
    };
    let source = NativeDataSource::new(data_dir);
    pollster::block_on(mode_a_web::run(
        patch,
        &options,
        &source,
        case.category,
        None,
    ))
    .expect("mixed-equipment migration")
    .remove(0)
}

fn synthetic_patch(unit_ids: &[u64]) -> PatchBytes {
    let mut patch = StreamToc {
        name: "mixed.patch_0".to_string(),
        entries: unit_ids
            .iter()
            .map(|file_id| TocEntry::new(*file_id, UNIT_ID))
            .collect(),
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
    patch
        .entries
        .iter()
        .any(|entry| entry.type_id == UNIT_ID && entry.file_id == file_id)
}

fn assert_mixed_warning(result: &mode_a_web::WebTargetResult, category: &str) {
    assert!(
        result
            .report
            .warnings
            .iter()
            .any(|warning| warning.contains(&format!("{category} "))
                && warning.contains("this patch may also contain")),
        "missing mixed {category} warning: {:?}",
        result.report.warnings
    );
}
