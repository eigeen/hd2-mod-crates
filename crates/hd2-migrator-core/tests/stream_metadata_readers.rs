use hd2_archive_format::stream_metadata::POISON_STREAM_SIZE;
use hd2_migrator_core::archive::sidecar::repairable_patch_sidecar_requirements;
use hd2_migrator_core::archive::toc_only::{TocOnlyEntry, TocOnlyPackage};
use hd2_migrator_core::archive::{StreamToc, TocFileType};
use hd2_migrator_core::constants::{BONE_ID, TEX_ID, UNIT_ID};
use hd2_migrator_core::io::NativeDataSource;
use hd2_migrator_core::operations::{
    MissingUnitPolicy, PatchBytes, UnitRepatchOptions, repatch_patch_with_progress, repatch_units,
};

#[test]
fn all_full_readers_resolve_metadata_before_slicing() {
    let input = fixture();
    let (borrowed, repairs) = StreamToc::from_buffers_with_repairs(
        &input.toc,
        &input.gpu,
        &input.stream,
        input.name.clone(),
    )
    .unwrap();
    assert_eq!(repairs.len(), 3);
    assert_archive_payloads(&borrowed);
    assert_archive_payloads(
        &StreamToc::from_buffers(&input.toc, &input.gpu, &input.stream, input.name.clone())
            .unwrap(),
    );
    assert_archive_payloads(
        &StreamToc::from_owned_buffers(
            input.toc.clone(),
            input.gpu.clone(),
            input.stream.clone(),
            input.name.clone(),
        )
        .unwrap(),
    );

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(&input.name);
    std::fs::write(&path, &input.toc).unwrap();
    std::fs::write(path.with_extension("patch_0.gpu_resources"), &input.gpu).unwrap();
    std::fs::write(path.with_extension("patch_0.stream"), &input.stream).unwrap();
    assert_archive_payloads(&StreamToc::from_files(&path).unwrap());

    let portable = hd2_archive_format::StreamToc::from_buffers(
        &input.toc,
        &input.gpu,
        &input.stream,
        input.name,
    )
    .unwrap();
    for entry in portable.entries {
        assert_eq!(entry.stream_data, expected_stream(entry.type_id));
    }
}

#[test]
fn inspection_toc_editing_and_full_reading_agree_without_rewriting_input() {
    let input = fixture();
    let requirements =
        repairable_patch_sidecar_requirements(&input.toc, input.stream.len()).unwrap();
    assert_eq!((requirements.gpu, requirements.stream), (4, 40));
    let (package, repairs) =
        TocOnlyPackage::parse_with_stream_metadata(&input.toc, Some(input.stream.len())).unwrap();
    assert_eq!(repairs.len(), 3);
    for entry in &package.entries {
        assert_eq!(
            entry.stream_size as usize,
            expected_stream(entry.type_id).len()
        );
    }
    let raw = TocOnlyPackage::parse(&input.toc).unwrap();
    assert!(
        raw.entries
            .iter()
            .all(|entry| entry.stream_size == POISON_STREAM_SIZE)
    );

    let corrected = package.serialize().unwrap();
    let (mut archive, repairs) =
        StreamToc::from_buffers_with_repairs(&corrected, &input.gpu, &input.stream, input.name)
            .unwrap();
    assert!(
        repairs.is_empty(),
        "already-corrected metadata must not be repaired twice"
    );
    let (toc, _, _) = archive.serialize();
    let emitted = TocOnlyPackage::parse(&toc).unwrap();
    for entry in emitted.entries {
        assert_eq!(
            entry.stream_size as usize,
            expected_stream(entry.type_id).len()
        );
    }
}

#[test]
fn repatch_writes_metadata_repairs_even_when_units_are_kept_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let source = NativeDataSource::new(directory.path());
    let input = fixture();
    let output = pollster::block_on(repatch_patch_with_progress(
        input,
        UnitRepatchOptions {
            missing_unit_policy: MissingUnitPolicy::Keep,
        },
        &source,
        None,
    ))
    .unwrap();
    assert_eq!(output.summary.updated_units, 0);
    assert_eq!(output.summary.removed_units, 0);
    assert!(output.gpu.is_none());
    assert!(output.stream.is_none());
    assert_corrected_toc(&output.toc);
}

#[test]
fn toc_only_repatch_repairs_known_sizes_without_assuming_an_empty_stream() {
    let directory = tempfile::tempdir().unwrap();
    let source = NativeDataSource::new(directory.path());
    let input = fixture();
    let output = pollster::block_on(repatch_units(
        &input.name,
        &input.toc,
        UnitRepatchOptions {
            missing_unit_policy: MissingUnitPolicy::Keep,
        },
        &source,
    ))
    .unwrap();
    assert_corrected_toc(&output.toc);
}

#[test]
fn repatch_writes_corrected_texture_metadata_when_there_are_no_units() {
    let directory = tempfile::tempdir().unwrap();
    let source = NativeDataSource::new(directory.path());
    let mut input = fixture();
    let mut raw = TocOnlyPackage::parse(&input.toc).unwrap();
    raw.entries.retain(|entry| entry.type_id == TEX_ID);
    input.toc = raw.serialize().unwrap();
    let output = pollster::block_on(repatch_patch_with_progress(
        input,
        UnitRepatchOptions::default(),
        &source,
        None,
    ))
    .unwrap();
    assert_eq!(output.summary.unit_count, 0);
    assert_corrected_toc(&output.toc);
}

fn assert_corrected_toc(toc: &[u8]) {
    for entry in TocOnlyPackage::parse(toc).unwrap().entries {
        assert_eq!(
            entry.stream_size as usize,
            expected_stream(entry.type_id).len()
        );
    }
}

fn assert_archive_payloads(archive: &StreamToc) {
    for entry in &archive.entries {
        assert_eq!(entry.stream_data.as_ref(), expected_stream(entry.type_id));
        if entry.type_id == UNIT_ID {
            assert_eq!(entry.gpu_data.as_ref(), &[9; 4]);
        }
    }
}

fn expected_stream(type_id: u64) -> &'static [u8] {
    if type_id == TEX_ID { &[7; 32] } else { &[] }
}

fn fixture() -> PatchBytes {
    let entries = [UNIT_ID, BONE_ID, TEX_ID]
        .into_iter()
        .map(fixture_entry)
        .collect::<Vec<_>>();
    let package = TocOnlyPackage {
        types: entries
            .iter()
            .map(|entry| TocFileType::new(entry.type_id, 1))
            .collect(),
        entries,
        unknown: 0,
        unk4_data: [0; 56],
    };
    PatchBytes {
        name: "example.patch_0".to_owned(),
        toc: package.serialize().unwrap(),
        gpu: vec![9; 4],
        stream: vec![7; 40],
    }
}

fn fixture_entry(type_id: u64) -> TocOnlyEntry {
    let mut toc_data = vec![0; 20];
    toc_data[16..20].copy_from_slice(&32u32.to_le_bytes());
    TocOnlyEntry {
        file_id: type_id,
        type_id,
        unknown1: 0,
        unknown2: 0,
        unknown3: 16,
        unknown4: 64,
        toc_data,
        stream_offset: if type_id == TEX_ID { 8 } else { u64::MAX },
        gpu_offset: 0,
        stream_size: POISON_STREAM_SIZE,
        gpu_size: if type_id == UNIT_ID { 4 } else { 0 },
    }
}
