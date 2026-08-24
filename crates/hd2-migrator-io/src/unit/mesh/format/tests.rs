use super::*;

#[test]
fn conflicting_slots_get_a_stable_distinct_id() {
    let materials = vec![(7, 10)];
    let first = unique_slot(&materials, 7, 20);
    let second = unique_slot(&materials, 7, 20);
    assert_eq!(first, second);
    assert_ne!(first, 7);
}

#[test]
fn index_encoding_promotes_without_changing_values() {
    let values = vec![0, 65_535, 70_000];
    let encoded = encode_indices(&values, 1);
    assert_eq!(decode_index_bytes(&encoded, 1).unwrap(), values);
}

#[test]
fn static_head_section_is_appended_and_transformed() {
    let body = fixture_unit(7, 77, 0.0);
    let helmet = fixture_unit(8, 88, 2.0);
    let output = merge_visible_head_into_body(&body, &helmet, &HashMap::from([(88, 99)]))
        .expect("merge synthetic units");
    assert_eq!(output.merged_lods, vec![0]);

    let parsed = ParsedUnit::parse(&output.entry).expect("parse merged Unit");
    assert_eq!(parsed.meshes[0].sections.len(), 2);
    assert_eq!(parsed.streams[0].vertex_count, 6);
    assert_eq!(parsed.streams[0].index_count, 6);
    assert!(parsed.materials.contains(&(8, 99)));
    let appended = parsed.streams[0].vertex_offset as usize + 3 * 12;
    let x = f32::from_bits(LE::read_u32(&parsed.gpu[appended..]));
    assert_eq!(x, 2.0);
}

fn fixture_unit(slot: u32, material: u64, translation_x: f32) -> TocEntry {
    let transform_offset = 0x80;
    let transforms = transform_table(translation_x);
    let bone_offset = transform_offset + transforms.len();
    let bones = build_bone_table(bone_offset, &[]).unwrap();
    let stream_offset = bone_offset + bones.len();
    let streams = build_record_table(&[stream_record()], Some(0), stream_offset).unwrap();
    let mesh_offset = stream_offset + streams.len();
    let meshes = build_record_table(&[mesh_record(slot)], None, mesh_offset).unwrap();
    let material_offset = mesh_offset + meshes.len();
    let materials = build_material_table(&[(slot, material)]).unwrap();
    let ending_offset = material_offset + materials.len();
    let mut toc = vec![0u8; ending_offset + 8];
    LE::write_u32(&mut toc[VERSION_FIELD..], CURRENT_VERSION);
    write_top_offset(&mut toc, TRANSFORM_OFFSET_FIELD, transform_offset);
    write_top_offset(&mut toc, BONE_OFFSET_FIELD, bone_offset);
    write_top_offset(&mut toc, STREAM_OFFSET_FIELD, stream_offset);
    write_top_offset(&mut toc, MESH_OFFSET_FIELD, mesh_offset);
    write_top_offset(&mut toc, MATERIAL_OFFSET_FIELD, material_offset);
    write_top_offset(&mut toc, ENDING_OFFSET_FIELD, ending_offset);
    toc[transform_offset..bone_offset].copy_from_slice(&transforms);
    toc[bone_offset..stream_offset].copy_from_slice(&bones);
    toc[stream_offset..mesh_offset].copy_from_slice(&streams);
    toc[mesh_offset..material_offset].copy_from_slice(&meshes);
    toc[material_offset..ending_offset].copy_from_slice(&materials);
    LE::write_u64(&mut toc[ending_offset..], 1);

    let mut entry = TocEntry::new(1, crate::constants::UNIT_ID);
    entry.toc_data = toc;
    entry.gpu_data = fixture_gpu().into();
    entry
}

fn transform_table(translation_x: f32) -> Vec<u8> {
    let mut bytes = vec![0u8; 16 + 64 + 64 + 4 + 4];
    LE::write_u32(&mut bytes, 1);
    write_identity(&mut bytes[16..80], 0.0);
    write_identity(&mut bytes[80..144], translation_x);
    LE::write_u32(&mut bytes[148..], 42);
    bytes
}

fn write_identity(bytes: &mut [u8], translation_x: f32) {
    for index in [0, 5, 10, 15] {
        LE::write_u32(&mut bytes[index * 4..], 1.0f32.to_bits());
    }
    LE::write_u32(&mut bytes[12 * 4..], translation_x.to_bits());
}

fn stream_record() -> RawRecord {
    let mut bytes = vec![0u8; STREAM_MIN_SIZE];
    LE::write_u32(&mut bytes[8..], 0);
    LE::write_u32(&mut bytes[12..], 2);
    LE::write_u64(&mut bytes[328..], 1);
    LE::write_u32(&mut bytes[352..], 3);
    LE::write_u32(&mut bytes[356..], 12);
    LE::write_u32(&mut bytes[392..], 3);
    LE::write_u32(&mut bytes[396..], 0);
    LE::write_u32(&mut bytes[416..], 0);
    LE::write_u32(&mut bytes[420..], 36);
    LE::write_u32(&mut bytes[424..], 48);
    LE::write_u32(&mut bytes[428..], 6);
    RawRecord { marker: 0, bytes }
}

fn mesh_record(slot: u32) -> RawRecord {
    let mut mesh = MeshRecord {
        raw: RawRecord {
            marker: 0,
            bytes: vec![0u8; MESH_MIN_SIZE],
        },
        dirty: true,
        transform_index: 0,
        lod: 0,
        stream_index: 0,
        material_slots: vec![slot],
        sections: vec![MeshSection {
            material_index: 0,
            vertex_offset: 0,
            vertex_count: 3,
            index_offset: 0,
            index_count: 3,
            group_index: 0,
        }],
    };
    LE::write_i32(&mut mesh.raw.bytes[56..], 0);
    mesh.build_raw().unwrap()
}

fn fixture_gpu() -> Vec<u8> {
    let mut gpu = Vec::new();
    for position in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        gpu.extend(position.into_iter().flat_map(f32::to_le_bytes));
    }
    gpu.resize(48, 0);
    gpu.extend([0u16, 1, 2].into_iter().flat_map(u16::to_le_bytes));
    gpu
}

fn write_top_offset(toc: &mut [u8], field: usize, value: usize) {
    LE::write_u32(&mut toc[field..], value as u32);
}
