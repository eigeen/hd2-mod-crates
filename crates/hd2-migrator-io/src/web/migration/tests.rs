use super::*;
use crate::archive::TocEntry;
use crate::constants::UNIT_ID;

#[test]
fn lists_targets_from_builtin_index() {
    let targets = list_target_options("Armor").unwrap();

    assert!(!targets.is_empty(), "builtin index has Armor entries");
}

#[test]
fn lists_unexcluded_helmet_targets() {
    let targets = list_target_options("Helmet").unwrap();

    assert_eq!(targets.len(), 106);
    assert!(targets.iter().all(|target| !target.excluded));
    assert!(
        targets
            .iter()
            .any(|target| target.name == "TG-8 Sharpshooter")
    );
    assert!(
        targets
            .iter()
            .any(|target| target.name == "TG-122 Demo-Trooper")
    );
    assert!(
        targets
            .iter()
            .all(|target| target.name != "UF-84 Doubt Killer")
    );
    assert!(
        targets
            .iter()
            .all(|target| target.name != "O-44 Bonded Pilot")
    );
}

#[test]
fn detects_helmet_source_from_authoritative_unit() {
    let table = HelmetMappingTable::bundled().unwrap();
    let unit_id = table.unit_id("TG-8 Sharpshooter").unwrap();
    let patch = patch_bytes("patch", &[unit_id]);

    let detected = detect_source_archive("Helmet", &patch).unwrap().unwrap();

    assert_eq!(detected.name, "TG-8 Sharpshooter");
}

#[test]
fn detected_multi_archive_helmet_uses_its_logical_option() {
    let table = HelmetMappingTable::bundled().unwrap();
    let unit_id = table.unit_id("AF-91 Field Chemist").unwrap();
    let patch = patch_bytes("patch", &[unit_id]);

    let detected = detect_source_archive("Helmet", &patch).unwrap().unwrap();
    let targets = list_target_options("Helmet").unwrap();

    assert_eq!(detected.name, "AF-91 Field Chemist");
    assert!(targets.iter().any(|target| target.hash == detected.hash));
}

#[test]
fn detect_source_returns_none_when_patch_has_no_known_units() {
    let patch = patch_bytes("patch", &[0xDEADBEEF]);

    let detected = detect_source_archive("Armor", &patch).unwrap();

    assert!(detected.is_none());
}

#[test]
fn detects_unique_models_across_armor_and_helmet_tables() {
    let armor = ArmorMappingTable::bundled().unwrap();
    let helmet = HelmetMappingTable::bundled().unwrap();
    let armor_unit = first_authoritative_file_id(&armor, "I-102 Draconaught");
    let helmet_unit = helmet.unit_id("A-9 Helljumper").unwrap();
    let patch = patch_bytes("patch", &[armor_unit, helmet_unit]);

    let models = detect_patch_models(&patch).unwrap();

    assert!(
        models
            .iter()
            .any(|model| { model.category == "Armor" && model.name == "I-102 Draconaught" })
    );
    assert!(
        models
            .iter()
            .any(|model| { model.category == "Helmet" && model.name == "A-9 Helljumper" })
    );
}

#[test]
fn ignores_units_reused_by_multiple_model_objects() {
    let shared = ModelKey {
        category: "Armor".to_string(),
        name: "Shared Armor".to_string(),
    };
    let other_shared = ModelKey {
        category: "Helmet".to_string(),
        name: "Shared Helmet".to_string(),
    };
    let unique = ModelKey {
        category: "Armor".to_string(),
        name: "Unique Armor".to_string(),
    };
    let owners = HashMap::from([
        (1, HashSet::from([shared, other_shared])),
        (2, HashSet::from([unique])),
    ]);

    let models = unique_model_hits(&owners, &HashSet::from([1, 2]));

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].name, "Unique Armor");
    assert_eq!(models[0].unit_hits, 1);
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
