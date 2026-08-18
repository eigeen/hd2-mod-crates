use hd2_migrator_io::archive::stream_metadata::normalize_patch_stream_metadata;
use hd2_migrator_io::archive::toc_only::TocOnlyPackage;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const MOCK_BAD_STREAM_SIZE: u32 = 0xDFA6_4C92;

#[test]
fn restores_mocked_stream_sizes_to_real_mod_values() {
    let patch_paths = local_patch_paths();
    if patch_paths.is_empty() {
        eprintln!("testdata/test_files is unavailable; skipping local real-mod regression");
        return;
    }
    let patch_count = patch_paths.len();
    let mut resource_count = 0;
    for patch_path in &patch_paths {
        resource_count += assert_mocked_sizes_restore(patch_path);
    }
    eprintln!("verified {patch_count} real mod patches and {resource_count} resource declarations");
}

/// Corrupt every declaration in memory, then compare repairs with the source TOC.
fn assert_mocked_sizes_restore(patch_path: &Path) -> usize {
    let stream_len = sidecar_len(patch_path, ".stream");
    let original_toc = fs::read(patch_path).expect("read real mod TOC fixture");
    let original = TocOnlyPackage::parse(&original_toc).expect("parse real mod TOC fixture");
    assert_normal_metadata_is_unchanged(&original_toc, stream_len);

    let mut mocked = original.clone();
    for entry in &mut mocked.entries {
        entry.stream_size = MOCK_BAD_STREAM_SIZE;
    }
    let mut mocked_toc = mocked.serialize().expect("serialize mocked real mod TOC");
    let repairs = normalize_patch_stream_metadata(&mut mocked_toc, stream_len)
        .expect("repair mocked real mod stream sizes");
    assert_eq!(repairs.len(), original.entries.len());
    assert_restored_metadata(&original, &mocked_toc);
    original.entries.len()
}

fn assert_normal_metadata_is_unchanged(original_toc: &[u8], stream_len: usize) {
    let mut normalized = original_toc.to_vec();
    let repairs = normalize_patch_stream_metadata(&mut normalized, stream_len)
        .expect("validate original real mod stream sizes");
    assert!(repairs.is_empty());
    assert_eq!(normalized, original_toc);
}

fn assert_restored_metadata(original: &TocOnlyPackage, repaired_toc: &[u8]) {
    let expected = stream_metadata_by_resource(original);
    let repaired = TocOnlyPackage::parse(repaired_toc).expect("parse repaired real mod TOC");
    assert_eq!(stream_metadata_by_resource(&repaired), expected);
}

fn stream_metadata_by_resource(package: &TocOnlyPackage) -> BTreeMap<(u64, u64), (u64, u32)> {
    package
        .entries
        .iter()
        .map(|entry| {
            (
                (entry.type_id, entry.file_id),
                (entry.stream_offset, entry.stream_size),
            )
        })
        .collect()
}

fn sidecar_len(patch_path: &Path, suffix: &str) -> usize {
    let sidecar_name = format!(
        "{}{}",
        patch_path.file_name().unwrap().to_string_lossy(),
        suffix
    );
    let sidecar_path = patch_path.with_file_name(sidecar_name);
    let length = fs::metadata(sidecar_path)
        .expect("read real mod sidecar metadata")
        .len();
    usize::try_from(length).expect("real mod sidecar length fits usize")
}

fn local_patch_paths() -> Vec<PathBuf> {
    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut patches = Vec::new();
    for directory_name in ["testdata", "test_files"] {
        collect_patch_paths(&repository_root.join(directory_name), &mut patches);
    }
    patches.sort();
    patches
}

fn collect_patch_paths(directory: &Path, patches: &mut Vec<PathBuf>) {
    let Ok(items) = fs::read_dir(directory) else {
        return;
    };
    for item in items.map(|item| item.expect("read local testdata entry")) {
        let path = item.path();
        if path.is_dir() {
            collect_patch_paths(&path, patches);
        } else if is_main_patch(&path) {
            patches.push(path);
        }
    }
}

fn is_main_patch(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.contains(".patch_")
                && !name.ends_with(".stream")
                && !name.ends_with(".gpu_resources")
        })
}
