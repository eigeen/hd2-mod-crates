use super::*;
use crate::archive::TocEntry;
use crate::constants::UNIT_ID;
use crate::web::metadata::WebArchiveMetadata;

#[test]
fn lists_targets_from_metadata() {
    let metadata =
        WebGameMetadata::new("Armor", vec![archive_metadata("source", "Source", &[1, 2])]);

    let targets = list_target_options(&metadata);

    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].name, "Source");
}

#[test]
fn detects_source_by_unit_overlap() {
    let metadata = WebGameMetadata::new(
        "Armor",
        vec![
            archive_metadata("a", "Weak", &[1]),
            archive_metadata("b", "Strong", &[1, 2]),
        ],
    );
    let patch = patch_bytes("patch", &[1, 2]);

    let detected = detect_source_archive(&metadata, &patch).unwrap().unwrap();

    assert_eq!(detected.hash, "b");
}

#[test]
fn detects_source_with_stable_tie_breaker() {
    let metadata = WebGameMetadata::new(
        "Armor",
        vec![
            archive_metadata("b", "Later Hash", &[1]),
            archive_metadata("a", "Earlier Hash", &[1]),
        ],
    );
    let patch = patch_bytes("patch", &[1]);

    let detected = detect_source_archive(&metadata, &patch).unwrap().unwrap();

    assert_eq!(detected.hash, "a");
}

#[test]
fn detects_source_from_toc_when_gpu_body_is_unavailable() {
    let metadata = WebGameMetadata::new("Armor", vec![archive_metadata("source", "Source", &[1])]);
    let patch = patch_bytes_without_gpu("patch", &[1]);

    let detected = detect_source_archive(&metadata, &patch).unwrap().unwrap();

    assert_eq!(detected.hash, "source");
}

#[test]
fn migrate_one_requires_one_target() {
    let metadata = WebGameMetadata::new("Armor", Vec::new());
    let err = migrate_one(
        &metadata,
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

fn archive_metadata(hash: &str, name: &str, unit_ids: &[u64]) -> WebArchiveMetadata {
    WebArchiveMetadata::from_stream(
        hash.to_string(),
        name.to_string(),
        &archive_stream(name, unit_ids),
    )
}

fn patch_bytes(name: &str, unit_ids: &[u64]) -> PatchBytes {
    let mut archive = archive_stream(name, unit_ids);
    let (toc, gpu, stream) = archive.serialize();
    PatchBytes {
        name: name.to_string(),
        toc,
        gpu,
        stream,
    }
}

fn patch_bytes_without_gpu(name: &str, unit_ids: &[u64]) -> PatchBytes {
    let mut patch = patch_bytes_with_gpu(name, unit_ids);
    patch.gpu.clear();
    patch
}

fn patch_bytes_with_gpu(name: &str, unit_ids: &[u64]) -> PatchBytes {
    let mut archive = archive_stream_with_gpu(name, unit_ids);
    let (toc, gpu, stream) = archive.serialize();
    PatchBytes {
        name: name.to_string(),
        toc,
        gpu,
        stream,
    }
}

fn archive_stream(name: &str, unit_ids: &[u64]) -> StreamToc {
    StreamToc {
        entries: unit_ids
            .iter()
            .map(|file_id| TocEntry::new(*file_id, UNIT_ID))
            .collect(),
        name: name.to_string(),
        ..Default::default()
    }
}

fn archive_stream_with_gpu(name: &str, unit_ids: &[u64]) -> StreamToc {
    let mut archive = archive_stream(name, unit_ids);
    for entry in &mut archive.entries {
        entry.gpu_data = vec![7, 8, 9];
    }
    archive
}
