use super::*;
use crate::archive::TocEntry;
use crate::constants::UNIT_ID;

#[test]
fn lists_targets_from_builtin_index() {
    let targets = list_target_options("Armor").unwrap();

    assert!(!targets.is_empty(), "builtin index has Armor entries");
}

#[test]
fn detect_source_returns_none_when_patch_has_no_known_units() {
    let patch = patch_bytes("patch", &[0xDEADBEEF]);

    let detected = detect_source_archive("Armor", &patch).unwrap();

    assert!(detected.is_none());
}

#[test]
fn migrate_one_requires_one_target() {
    let err = migrate_one(
        "Armor",
        patch_bytes("patch", &[1]),
        WebMigrateOptions {
            source_hash: None,
            target_hashes: Vec::new(),
            patch_suffix: None,
            no_padding: true,
            experimental_partial_remap: false,
        },
    )
    .unwrap_err();

    assert!(err.to_string().contains("exactly one target"));
}

#[test]
fn migrate_cross_archive_is_unavailable_in_browser() {
    let table = ArmorMappingTable::bundled().unwrap();
    let by_hash = archive_name_lookup("Armor").unwrap();
    let (source_entry, target_entry) = pick_two_authoritative_armors(&table, &by_hash);
    let source_unit_id = first_authoritative_file_id(&table, &source_entry.1);
    let patch = patch_bytes("patch", &[source_unit_id]);

    let err = migrate_one(
        "Armor",
        patch,
        WebMigrateOptions {
            source_hash: Some(source_entry.0.clone()),
            target_hashes: vec![target_entry.0.clone()],
            patch_suffix: None,
            no_padding: true,
            experimental_partial_remap: false,
        },
    )
    .unwrap_err();

    assert!(err.to_string().contains("cross-archive migration is not available"));
}

fn pick_two_authoritative_armors(
    table: &ArmorMappingTable,
    by_hash: &[(String, String)],
) -> ((String, String), (String, String)) {
    let mut iter = by_hash
        .iter()
        .filter(|(_, name)| table.armor(name).is_some())
        .cloned();
    let source = iter.next().expect("at least one authoritative armor");
    let target = iter.next().expect("at least two authoritative armors");
    (source, target)
}

fn first_authoritative_file_id(table: &ArmorMappingTable, armor_name: &str) -> u64 {
    *table
        .armor(armor_name)
        .expect("armor in table")
        .all_file_ids()
        .first()
        .expect("non-empty part map")
}

fn patch_bytes(name: &str, unit_ids: &[u64]) -> PatchBytes {
    let mut archive = StreamToc {
        entries: unit_ids
            .iter()
            .map(|file_id| TocEntry::new(*file_id, UNIT_ID))
            .collect(),
        name: name.to_string(),
        ..Default::default()
    };
    let (toc, gpu, stream) = archive.serialize();
    PatchBytes {
        name: name.to_string(),
        toc,
        gpu,
        stream,
    }
}
