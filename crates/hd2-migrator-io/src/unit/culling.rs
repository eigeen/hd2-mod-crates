//! Culling MeshInfo recognition and raw-preserving Unit composition.

use crate::archive::TocEntry;
use byteorder::{ByteOrder, LittleEndian as LE};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

const TRANSFORM_OFFSET_FIELD: usize = 0x34;
const BONE_INFO_OFFSET_FIELD: usize = 0x58;
const STREAM_INFO_OFFSET_FIELD: usize = 0x5c;
const ENDING_OFFSET_FIELD: usize = 0x60;
const MESH_INFO_OFFSET_FIELD: usize = 0x64;
const MATERIALS_OFFSET_FIELD: usize = 0x70;
const TOP_LEVEL_OFFSET_START: usize = 0x30;
const TOP_LEVEL_OFFSET_END: usize = 0x70;
const STREAM_RECORD_MIN_SIZE: usize = 432;
const STREAM_VERTEX_OFFSET: usize = 416;
const STREAM_VERTEX_SIZE: usize = 420;
const STREAM_INDEX_OFFSET: usize = 424;
const STREAM_INDEX_SIZE: usize = 428;
const MESH_TRANSFORM_INDEX: usize = 48;
const MESH_LOD_INDEX: usize = 56;
const MESH_STREAM_INDEX: usize = 60;
const MESH_NUM_MATERIALS: usize = 104;
const MESH_MATERIAL_OFFSET: usize = 108;
const MESH_NUM_SECTIONS: usize = 120;
const MESH_SECTIONS_OFFSET: usize = 124;
const MESH_SECTION_SIZE: usize = 24;
const GPU_ALIGNMENT: usize = 16;
const TRANSFORM_HEADER_SIZE: usize = 16;
const LOCAL_TRANSFORM_SIZE: usize = 64;
const TRANSFORM_MATRIX_SIZE: usize = 64;
const TRANSFORM_ENTRY_SIZE: usize = 4;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CullingPolicy {
    #[default]
    Patch,
    Target,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CullingMesh {
    pub mesh_index: usize,
    pub stream_index: usize,
    pub transform_index: usize,
    pub lod_index: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CullingInspection {
    pub mesh_count: usize,
    pub culling_meshes: Vec<CullingMesh>,
}

#[derive(Debug, Clone)]
struct RawRecord {
    marker: u32,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
struct ParsedMesh {
    record: RawRecord,
    stream_index: usize,
    transform_index: usize,
    lod_index: i32,
    is_culling: bool,
}

#[derive(Debug)]
struct ParsedUnit {
    stream_offset: usize,
    materials_offset: usize,
    stream_unknown: u32,
    streams: Vec<RawRecord>,
    meshes: Vec<ParsedMesh>,
}

pub fn inspect_unit_culling(unit: &[u8]) -> crate::Result<CullingInspection> {
    let parsed = parse_unit(unit)?;
    let culling_meshes = parsed
        .meshes
        .iter()
        .enumerate()
        .filter(|(_, mesh)| mesh.is_culling)
        .map(|(mesh_index, mesh)| culling_mesh(mesh_index, mesh))
        .collect();
    Ok(CullingInspection {
        mesh_count: parsed.meshes.len(),
        culling_meshes,
    })
}

/// Keep the patch Unit as the model base while replacing its entire culling set.
pub fn replace_patch_culling_with_target(
    patch: &TocEntry,
    target: &TocEntry,
) -> crate::Result<TocEntry> {
    let target_unit = parse_unit(&target.toc_data)?;
    let dependencies = import_target_dependencies(&patch.toc_data, &target.toc_data, &target_unit)?;
    let merged_source = parse_unit(&dependencies.toc_data)?;
    let import = prepare_target_import(patch, target, &merged_source, &target_unit, dependencies)?;
    let toc_data = rebuild_unit(&import.base_toc, &merged_source, &target_unit, &import)?;
    let mut output = patch.clone();
    output.toc_data = toc_data;
    output.gpu_data = import.gpu_data.into();
    validate_replacement(&output.toc_data, &merged_source, &target_unit)?;
    Ok(output)
}

fn culling_mesh(mesh_index: usize, mesh: &ParsedMesh) -> CullingMesh {
    CullingMesh {
        mesh_index,
        stream_index: mesh.stream_index,
        transform_index: mesh.transform_index,
        lod_index: mesh.lod_index,
    }
}

fn parse_unit(unit: &[u8]) -> crate::Result<ParsedUnit> {
    require_range(unit, MATERIALS_OFFSET_FIELD, 4, "Unit header")?;
    let stream_offset = required_offset(unit, STREAM_INFO_OFFSET_FIELD, "StreamInfo")?;
    let mesh_offset = required_offset(unit, MESH_INFO_OFFSET_FIELD, "MeshInfo")?;
    let materials_offset = required_offset(unit, MATERIALS_OFFSET_FIELD, "materials")?;
    validate_table_order(unit, stream_offset, mesh_offset, materials_offset)?;
    let normal_slots = read_normal_material_slots(unit, materials_offset)?;
    let (streams, stream_unknown) = read_stream_records(unit, stream_offset, mesh_offset)?;
    let meshes = read_mesh_records(unit, mesh_offset, materials_offset, &normal_slots)?;
    Ok(ParsedUnit {
        stream_offset,
        materials_offset,
        stream_unknown,
        streams,
        meshes,
    })
}

fn validate_table_order(
    unit: &[u8],
    stream_offset: usize,
    mesh_offset: usize,
    materials_offset: usize,
) -> crate::Result<()> {
    if stream_offset >= mesh_offset
        || mesh_offset >= materials_offset
        || materials_offset >= unit.len()
    {
        eyre::bail!(
            "unsupported Unit table order: stream={stream_offset}, mesh={mesh_offset}, materials={materials_offset}"
        );
    }
    Ok(())
}

fn read_normal_material_slots(unit: &[u8], offset: usize) -> crate::Result<HashSet<u32>> {
    let count = read_u32(unit, offset, "material count")? as usize;
    let slots_offset = offset + 4;
    require_range(unit, slots_offset, count * 4, "material slot table")?;
    Ok((0..count)
        .map(|index| LE::read_u32(&unit[slots_offset + index * 4..]))
        .collect())
}

fn read_stream_records(
    unit: &[u8],
    table_offset: usize,
    table_end: usize,
) -> crate::Result<(Vec<RawRecord>, u32)> {
    let (offsets, markers, trailer_offset) =
        read_record_table_header(unit, table_offset, table_end)?;
    let unknown = read_u32(unit, trailer_offset, "StreamInfo table trailer")?;
    let records = raw_records(
        unit,
        table_offset,
        table_end,
        &offsets,
        &markers,
        STREAM_RECORD_MIN_SIZE,
    )?;
    Ok((records, unknown))
}

fn read_mesh_records(
    unit: &[u8],
    table_offset: usize,
    table_end: usize,
    normal_slots: &HashSet<u32>,
) -> crate::Result<Vec<ParsedMesh>> {
    let (offsets, markers, _) = read_record_table_header(unit, table_offset, table_end)?;
    let records = raw_records(unit, table_offset, table_end, &offsets, &markers, 128)?;
    records
        .into_iter()
        .map(|record| parse_mesh(record, normal_slots))
        .collect()
}

fn parse_mesh(record: RawRecord, normal_slots: &HashSet<u32>) -> crate::Result<ParsedMesh> {
    let bytes = &record.bytes;
    require_range(bytes, MESH_SECTIONS_OFFSET, 4, "MeshInfo header")?;
    let material_slots = read_mesh_material_slots(bytes)?;
    let section_slots = read_mesh_section_slots(bytes, &material_slots)?;
    Ok(ParsedMesh {
        stream_index: read_u32(bytes, MESH_STREAM_INDEX, "MeshInfo stream index")? as usize,
        transform_index: read_u32(bytes, MESH_TRANSFORM_INDEX, "MeshInfo transform index")?
            as usize,
        lod_index: read_i32(bytes, MESH_LOD_INDEX, "MeshInfo LOD index")?,
        is_culling: !section_slots.is_empty()
            && material_slots
                .iter()
                .all(|slot| !normal_slots.contains(slot)),
        record,
    })
}

fn read_mesh_material_slots(mesh: &[u8]) -> crate::Result<Vec<u32>> {
    let count = read_u32(mesh, MESH_NUM_MATERIALS, "MeshInfo material count")? as usize;
    let offset = read_u32(mesh, MESH_MATERIAL_OFFSET, "MeshInfo material offset")? as usize;
    require_range(mesh, offset, count * 4, "MeshInfo material slots")?;
    Ok((0..count)
        .map(|index| LE::read_u32(&mesh[offset + index * 4..]))
        .collect())
}

fn read_mesh_section_slots(mesh: &[u8], material_slots: &[u32]) -> crate::Result<Vec<u32>> {
    let count = read_u32(mesh, MESH_NUM_SECTIONS, "MeshInfo section count")? as usize;
    let offset = read_u32(mesh, MESH_SECTIONS_OFFSET, "MeshInfo section offset")? as usize;
    require_range(mesh, offset, count * MESH_SECTION_SIZE, "MeshInfo sections")?;
    (0..count)
        .map(|index| section_slot(mesh, offset, index, material_slots))
        .collect()
}

fn section_slot(mesh: &[u8], offset: usize, index: usize, slots: &[u32]) -> crate::Result<u32> {
    let material_index = LE::read_u32(&mesh[offset + index * MESH_SECTION_SIZE..]) as usize;
    slots.get(material_index).copied().ok_or_else(|| {
        eyre::eyre!("MeshInfo section material index {material_index} is out of bounds")
    })
}

fn read_record_table_header(
    unit: &[u8],
    table_offset: usize,
    table_end: usize,
) -> crate::Result<(Vec<usize>, Vec<u32>, usize)> {
    let count = read_u32(unit, table_offset, "record table count")? as usize;
    let offsets_start = table_offset + 4;
    let markers_start = offsets_start + count * 4;
    require_range(unit, offsets_start, count * 8, "record table arrays")?;
    let offsets = (0..count)
        .map(|index| LE::read_u32(&unit[offsets_start + index * 4..]) as usize)
        .collect::<Vec<_>>();
    let markers = (0..count)
        .map(|index| LE::read_u32(&unit[markers_start + index * 4..]))
        .collect::<Vec<_>>();
    validate_record_offsets(&offsets, table_offset, table_end)?;
    Ok((offsets, markers, markers_start + count * 4))
}

fn validate_record_offsets(offsets: &[usize], table: usize, end: usize) -> crate::Result<()> {
    let mut previous = 0usize;
    for offset in offsets {
        if *offset <= previous || table + *offset >= end {
            eyre::bail!("record table contains an invalid relative offset {offset}");
        }
        previous = *offset;
    }
    Ok(())
}

fn raw_records(
    unit: &[u8],
    table: usize,
    table_end: usize,
    offsets: &[usize],
    markers: &[u32],
    minimum_size: usize,
) -> crate::Result<Vec<RawRecord>> {
    offsets
        .iter()
        .enumerate()
        .map(|(index, offset)| {
            raw_record(
                unit,
                table,
                table_end,
                offsets,
                markers,
                index,
                *offset,
                minimum_size,
            )
        })
        .collect()
}

fn raw_record(
    unit: &[u8],
    table: usize,
    table_end: usize,
    offsets: &[usize],
    markers: &[u32],
    index: usize,
    offset: usize,
    minimum_size: usize,
) -> crate::Result<RawRecord> {
    let start = table + offset;
    let end = offsets
        .get(index + 1)
        .map(|next| table + *next)
        .unwrap_or(table_end);
    require_range(unit, start, end.saturating_sub(start), "record body")?;
    if end < start + minimum_size {
        eyre::bail!("record {index} is shorter than {minimum_size} bytes");
    }
    Ok(RawRecord {
        marker: markers[index],
        bytes: unit[start..end].to_vec(),
    })
}

#[derive(Clone)]
struct TransformRecord {
    local: Vec<u8>,
    matrix: Vec<u8>,
    entry: Vec<u8>,
    name_hash: u32,
}

struct TransformTable {
    offset: usize,
    end: usize,
    header: Vec<u8>,
    records: Vec<TransformRecord>,
}

struct BoneTable {
    offset: usize,
    end: usize,
    records: Vec<Vec<u8>>,
}

fn import_target_dependencies(
    source: &[u8],
    target: &[u8],
    target_unit: &ParsedUnit,
) -> crate::Result<DependencyImport> {
    let target_bones = selected_target_bones(target, target_unit)?;
    let transform_indexes = selected_transform_indexes(target, target_unit, &target_bones)?;
    let (toc_data, transform_map) = merge_target_transforms(source, target, &transform_indexes)?;
    let (toc_data, bone_map) = merge_target_bones(toc_data, &target_bones, &transform_map)?;
    validate_dependency_maps(target_unit, &transform_map, &bone_map)?;
    Ok(DependencyImport {
        toc_data,
        transform_map,
        bone_map,
    })
}

fn selected_target_bones(
    target: &[u8],
    target_unit: &ParsedUnit,
) -> crate::Result<Vec<(usize, Vec<u8>)>> {
    let indexes = target_unit
        .meshes
        .iter()
        .filter(|mesh| mesh.is_culling && mesh.lod_index >= 0)
        .map(|mesh| mesh.lod_index as usize)
        .collect::<BTreeSet<_>>();
    if indexes.is_empty() {
        return Ok(Vec::new());
    }
    let table = parse_bone_table(target)?;
    indexes
        .into_iter()
        .map(|index| {
            table
                .records
                .get(index)
                .cloned()
                .map(|record| (index, record))
                .ok_or_else(|| eyre::eyre!("target culling references missing bone info {index}"))
        })
        .collect()
}

fn selected_transform_indexes(
    target: &[u8],
    target_unit: &ParsedUnit,
    bones: &[(usize, Vec<u8>)],
) -> crate::Result<BTreeSet<usize>> {
    let mut indexes = target_unit
        .meshes
        .iter()
        .filter(|mesh| mesh.is_culling)
        .map(|mesh| mesh.transform_index)
        .collect::<BTreeSet<_>>();
    for (_, bone) in bones {
        indexes.extend(bone_real_indices(bone)?);
    }
    if indexes.is_empty() {
        return Ok(indexes);
    }
    let transforms = parse_transform_table(target)?;
    include_transform_parents(&mut indexes, &transforms)?;
    Ok(indexes)
}

fn include_transform_parents(
    indexes: &mut BTreeSet<usize>,
    table: &TransformTable,
) -> crate::Result<()> {
    let mut pending = indexes.iter().copied().collect::<Vec<_>>();
    while let Some(index) = pending.pop() {
        let record = table
            .records
            .get(index)
            .ok_or_else(|| eyre::eyre!("target culling references missing transform {index}"))?;
        let parent = LE::read_u16(&record.entry[2..]) as usize;
        if parent < table.records.len() && indexes.insert(parent) {
            pending.push(parent);
        }
    }
    Ok(())
}

fn merge_target_transforms(
    source: &[u8],
    target: &[u8],
    indexes: &BTreeSet<usize>,
) -> crate::Result<(Vec<u8>, HashMap<usize, usize>)> {
    if indexes.is_empty() {
        return Ok((source.to_vec(), HashMap::new()));
    }
    let source_table = parse_transform_table(source)?;
    let target_table = parse_transform_table(target)?;
    let mut records = source_table.records.clone();
    let mut by_hash = transform_indexes_by_hash(&records);
    let mut mapping = HashMap::new();
    let mut appended = Vec::new();
    for target_index in indexes {
        let target_record = target_table.records.get(*target_index).ok_or_else(|| {
            eyre::eyre!("target culling references missing transform {target_index}")
        })?;
        let output_index = by_hash
            .get(&target_record.name_hash)
            .copied()
            .unwrap_or_else(|| append_transform(&mut records, &mut by_hash, target_record));
        mapping.insert(*target_index, output_index);
        if output_index >= source_table.records.len() {
            appended.push((*target_index, output_index));
        }
    }
    remap_appended_transform_parents(&mut records, &appended, &target_table, &mapping)?;
    if records.len() == source_table.records.len() {
        return Ok((source.to_vec(), mapping));
    }
    let replacement = build_transform_table(&source_table, &records)?;
    Ok((
        splice_dependency_table(source, source_table.offset, source_table.end, &replacement)?,
        mapping,
    ))
}

fn transform_indexes_by_hash(records: &[TransformRecord]) -> HashMap<u32, usize> {
    records
        .iter()
        .enumerate()
        .map(|(index, record)| (record.name_hash, index))
        .collect()
}

fn append_transform(
    records: &mut Vec<TransformRecord>,
    by_hash: &mut HashMap<u32, usize>,
    record: &TransformRecord,
) -> usize {
    let index = records.len();
    records.push(record.clone());
    by_hash.insert(record.name_hash, index);
    index
}

fn remap_appended_transform_parents(
    records: &mut [TransformRecord],
    appended: &[(usize, usize)],
    target: &TransformTable,
    mapping: &HashMap<usize, usize>,
) -> crate::Result<()> {
    for (target_index, output_index) in appended {
        let parent = LE::read_u16(&target.records[*target_index].entry[2..]) as usize;
        if parent >= target.records.len() {
            continue;
        }
        let mapped = mapping.get(&parent).ok_or_else(|| {
            eyre::eyre!("target transform {target_index} has unmapped parent {parent}")
        })?;
        LE::write_u16(
            &mut records[*output_index].entry[2..],
            u16::try_from(*mapped)?,
        );
    }
    Ok(())
}

fn merge_target_bones(
    source: Vec<u8>,
    target_bones: &[(usize, Vec<u8>)],
    transform_map: &HashMap<usize, usize>,
) -> crate::Result<(Vec<u8>, HashMap<usize, usize>)> {
    if target_bones.is_empty() {
        return Ok((source, HashMap::new()));
    }
    let source_table = parse_bone_table(&source)?;
    let mut records = source_table.records.clone();
    let mut bone_map = HashMap::new();
    for (target_index, record) in target_bones {
        let output_index = records.len();
        records.push(remap_bone_transforms(record.clone(), transform_map)?);
        bone_map.insert(*target_index, output_index);
    }
    let replacement = build_bone_table(source_table.offset, &records)?;
    let output =
        splice_dependency_table(&source, source_table.offset, source_table.end, &replacement)?;
    Ok((output, bone_map))
}

fn validate_dependency_maps(
    target: &ParsedUnit,
    transforms: &HashMap<usize, usize>,
    bones: &HashMap<usize, usize>,
) -> crate::Result<()> {
    for mesh in target.meshes.iter().filter(|mesh| mesh.is_culling) {
        if !transforms.contains_key(&mesh.transform_index) {
            eyre::bail!(
                "target culling transform {} was not imported",
                mesh.transform_index
            );
        }
        if mesh.lod_index >= 0 && !bones.contains_key(&(mesh.lod_index as usize)) {
            eyre::bail!(
                "target culling bone info {} was not imported",
                mesh.lod_index
            );
        }
    }
    Ok(())
}

fn parse_transform_table(unit: &[u8]) -> crate::Result<TransformTable> {
    let offset = required_offset(unit, TRANSFORM_OFFSET_FIELD, "TransformInfo")?;
    let count = read_u32(unit, offset, "transform count")? as usize;
    let arrays_offset = offset + TRANSFORM_HEADER_SIZE;
    let local_offset = arrays_offset;
    let matrix_offset = local_offset + count * LOCAL_TRANSFORM_SIZE;
    let entry_offset = matrix_offset + count * TRANSFORM_MATRIX_SIZE;
    let hashes_offset = entry_offset + count * TRANSFORM_ENTRY_SIZE;
    let data_end = hashes_offset + count * 4;
    require_range(unit, offset, data_end - offset, "TransformInfo table")?;
    let records = (0..count)
        .map(|index| TransformRecord {
            local: unit[local_offset + index * LOCAL_TRANSFORM_SIZE
                ..local_offset + (index + 1) * LOCAL_TRANSFORM_SIZE]
                .to_vec(),
            matrix: unit[matrix_offset + index * TRANSFORM_MATRIX_SIZE
                ..matrix_offset + (index + 1) * TRANSFORM_MATRIX_SIZE]
                .to_vec(),
            entry: unit[entry_offset + index * TRANSFORM_ENTRY_SIZE
                ..entry_offset + (index + 1) * TRANSFORM_ENTRY_SIZE]
                .to_vec(),
            name_hash: LE::read_u32(&unit[hashes_offset + index * 4..]),
        })
        .collect();
    Ok(TransformTable {
        offset,
        end: align_up(data_end, 16),
        header: unit[offset..arrays_offset].to_vec(),
        records,
    })
}

fn build_transform_table(
    source: &TransformTable,
    records: &[TransformRecord],
) -> crate::Result<Vec<u8>> {
    let data_len = TRANSFORM_HEADER_SIZE
        + records.len() * (LOCAL_TRANSFORM_SIZE + TRANSFORM_MATRIX_SIZE + TRANSFORM_ENTRY_SIZE + 4);
    let table_len = align_up(source.offset + data_len, 16) - source.offset;
    let mut output = Vec::with_capacity(table_len);
    output.extend_from_slice(&source.header);
    LE::write_u32(&mut output[..4], u32::try_from(records.len())?);
    for record in records {
        output.extend_from_slice(&record.local);
    }
    for record in records {
        output.extend_from_slice(&record.matrix);
    }
    for record in records {
        output.extend_from_slice(&record.entry);
    }
    for record in records {
        output.extend_from_slice(&record.name_hash.to_le_bytes());
    }
    output.resize(table_len, 0);
    Ok(output)
}

fn parse_bone_table(unit: &[u8]) -> crate::Result<BoneTable> {
    let offset = required_offset(unit, BONE_INFO_OFFSET_FIELD, "BoneInfo")?;
    let end = required_offset(unit, STREAM_INFO_OFFSET_FIELD, "StreamInfo")?;
    if offset >= end {
        eyre::bail!("unsupported BoneInfo table range {offset}..{end}");
    }
    let count = read_u32(unit, offset, "BoneInfo count")? as usize;
    let offsets = (0..count)
        .map(|index| read_u32(unit, offset + 4 + index * 4, "BoneInfo offset").map(|v| v as usize))
        .collect::<crate::Result<Vec<_>>>()?;
    validate_record_offsets(&offsets, offset, end)?;
    let records = offsets
        .iter()
        .enumerate()
        .map(|(index, relative)| {
            let start = offset + relative;
            let record_end = offsets
                .get(index + 1)
                .map(|next| offset + next)
                .unwrap_or(end);
            unit.get(start..record_end)
                .map(<[u8]>::to_vec)
                .ok_or_else(|| eyre::eyre!("BoneInfo record {index} is out of bounds"))
        })
        .collect::<crate::Result<Vec<_>>>()?;
    Ok(BoneTable {
        offset,
        end,
        records,
    })
}

fn build_bone_table(offset: usize, records: &[Vec<u8>]) -> crate::Result<Vec<u8>> {
    let header_len = 4 + records.len() * 4;
    let mut output = vec![0u8; header_len];
    LE::write_u32(&mut output, u32::try_from(records.len())?);
    for (index, record) in records.iter().enumerate() {
        let record_offset = output.len();
        LE::write_u32(&mut output[4 + index * 4..], u32::try_from(record_offset)?);
        output.extend_from_slice(record);
    }
    let aligned_len = align_up(offset + output.len(), 16) - offset;
    output.resize(aligned_len, 0);
    Ok(output)
}

fn bone_real_indices(record: &[u8]) -> crate::Result<Vec<usize>> {
    let count = read_u32(record, 0, "BoneInfo bone count")? as usize;
    let offset = read_u32(record, 8, "BoneInfo real-index offset")? as usize;
    require_range(record, offset, count * 4, "BoneInfo real indices")?;
    Ok((0..count)
        .map(|index| LE::read_u32(&record[offset + index * 4..]) as usize)
        .collect())
}

fn remap_bone_transforms(
    mut record: Vec<u8>,
    transform_map: &HashMap<usize, usize>,
) -> crate::Result<Vec<u8>> {
    let count = read_u32(&record, 0, "BoneInfo bone count")? as usize;
    let offset = read_u32(&record, 8, "BoneInfo real-index offset")? as usize;
    require_range(&record, offset, count * 4, "BoneInfo real indices")?;
    for index in 0..count {
        let field = offset + index * 4;
        let target_index = LE::read_u32(&record[field..]) as usize;
        let output_index = transform_map
            .get(&target_index)
            .ok_or_else(|| eyre::eyre!("BoneInfo references unmapped transform {target_index}"))?;
        LE::write_u32(&mut record[field..], u32::try_from(*output_index)?);
    }
    Ok(record)
}

fn splice_dependency_table(
    source: &[u8],
    start: usize,
    end: usize,
    replacement: &[u8],
) -> crate::Result<Vec<u8>> {
    require_range(source, start, end.saturating_sub(start), "dependency table")?;
    let delta = replacement.len() as i64 - (end - start) as i64;
    let mut output = Vec::with_capacity((source.len() as i64 + delta) as usize);
    output.extend_from_slice(&source[..start]);
    output.extend_from_slice(replacement);
    output.extend_from_slice(&source[end..]);
    adjust_offsets_after(&mut output, end, delta)?;
    Ok(output)
}

fn adjust_offsets_after(output: &mut [u8], threshold: usize, delta: i64) -> crate::Result<()> {
    for field in (TOP_LEVEL_OFFSET_START..=TOP_LEVEL_OFFSET_END).step_by(4) {
        let value = read_u32(output, field, "Unit top-level offset")? as usize;
        if value >= threshold {
            write_adjusted_offset(output, field, value, delta)?;
        }
    }
    Ok(())
}

struct TargetImport {
    base_toc: Vec<u8>,
    stream_map: HashMap<usize, usize>,
    transform_map: HashMap<usize, usize>,
    bone_map: HashMap<usize, usize>,
    streams: Vec<RawRecord>,
    gpu_data: Vec<u8>,
}

struct DependencyImport {
    toc_data: Vec<u8>,
    transform_map: HashMap<usize, usize>,
    bone_map: HashMap<usize, usize>,
}

fn prepare_target_import(
    patch: &TocEntry,
    target: &TocEntry,
    source_unit: &ParsedUnit,
    target_unit: &ParsedUnit,
    dependencies: DependencyImport,
) -> crate::Result<TargetImport> {
    let indexes = target_culling_stream_indexes(target_unit);
    let mut gpu_data = patch.gpu_data.to_vec();
    let mut streams = Vec::new();
    let mut stream_map = HashMap::new();
    for target_index in indexes {
        let new_index = source_unit.streams.len() + streams.len();
        let stream = import_stream(target, target_unit, target_index, &mut gpu_data)?;
        stream_map.insert(target_index, new_index);
        streams.push(stream);
    }
    Ok(TargetImport {
        base_toc: dependencies.toc_data,
        stream_map,
        transform_map: dependencies.transform_map,
        bone_map: dependencies.bone_map,
        streams,
        gpu_data,
    })
}

fn target_culling_stream_indexes(unit: &ParsedUnit) -> Vec<usize> {
    let mut indexes = unit
        .meshes
        .iter()
        .filter(|mesh| mesh.is_culling)
        .map(|mesh| mesh.stream_index)
        .collect::<Vec<_>>();
    indexes.sort_unstable();
    indexes.dedup();
    indexes
}

fn import_stream(
    target: &TocEntry,
    unit: &ParsedUnit,
    index: usize,
    output_gpu: &mut Vec<u8>,
) -> crate::Result<RawRecord> {
    let mut record = unit
        .streams
        .get(index)
        .cloned()
        .ok_or_else(|| eyre::eyre!("target culling references missing StreamInfo {index}"))?;
    copy_gpu_range(
        &target.gpu_data,
        output_gpu,
        &mut record.bytes,
        STREAM_VERTEX_OFFSET,
        STREAM_VERTEX_SIZE,
    )?;
    copy_gpu_range(
        &target.gpu_data,
        output_gpu,
        &mut record.bytes,
        STREAM_INDEX_OFFSET,
        STREAM_INDEX_SIZE,
    )?;
    Ok(record)
}

fn copy_gpu_range(
    source: &[u8],
    output: &mut Vec<u8>,
    stream: &mut [u8],
    offset_field: usize,
    size_field: usize,
) -> crate::Result<()> {
    let offset = read_u32(stream, offset_field, "GPU buffer offset")? as usize;
    let size = read_u32(stream, size_field, "GPU buffer size")? as usize;
    require_range(source, offset, size, "target GPU buffer")?;
    let new_offset = append_aligned(output, &source[offset..offset + size]);
    LE::write_u32(&mut stream[offset_field..], u32::try_from(new_offset)?);
    Ok(())
}

fn append_aligned(output: &mut Vec<u8>, bytes: &[u8]) -> usize {
    let offset = align_up(output.len(), GPU_ALIGNMENT);
    output.resize(offset, 0);
    output.extend_from_slice(bytes);
    offset
}

fn rebuild_unit(
    source: &[u8],
    source_unit: &ParsedUnit,
    target_unit: &ParsedUnit,
    import: &TargetImport,
) -> crate::Result<Vec<u8>> {
    let streams = source_unit
        .streams
        .iter()
        .cloned()
        .chain(import.streams.iter().cloned())
        .collect::<Vec<_>>();
    let meshes = replacement_mesh_records(source_unit, target_unit, import)?;
    let stream_table = build_record_table(
        &streams,
        Some(source_unit.stream_unknown),
        source_unit.stream_offset,
    )?;
    let mesh_offset = source_unit.stream_offset + stream_table.len();
    let mesh_table = build_record_table(&meshes, None, mesh_offset)?;
    splice_unit_tables(source, source_unit, &stream_table, &mesh_table)
}

fn replacement_mesh_records(
    source: &ParsedUnit,
    target: &ParsedUnit,
    import: &TargetImport,
) -> crate::Result<Vec<RawRecord>> {
    let mut records = source
        .meshes
        .iter()
        .filter(|mesh| !mesh.is_culling)
        .map(|mesh| mesh.record.clone())
        .collect::<Vec<_>>();
    for mesh in target.meshes.iter().filter(|mesh| mesh.is_culling) {
        let mut record = mesh.record.clone();
        let stream_index = import
            .stream_map
            .get(&mesh.stream_index)
            .ok_or_else(|| eyre::eyre!("missing imported StreamInfo {}", mesh.stream_index))?;
        LE::write_u32(
            &mut record.bytes[MESH_STREAM_INDEX..],
            u32::try_from(*stream_index)?,
        );
        let transform_index = import
            .transform_map
            .get(&mesh.transform_index)
            .ok_or_else(|| eyre::eyre!("missing imported transform {}", mesh.transform_index))?;
        LE::write_u32(
            &mut record.bytes[MESH_TRANSFORM_INDEX..],
            u32::try_from(*transform_index)?,
        );
        if mesh.lod_index >= 0 {
            let bone_index = import
                .bone_map
                .get(&(mesh.lod_index as usize))
                .ok_or_else(|| eyre::eyre!("missing imported bone info {}", mesh.lod_index))?;
            LE::write_i32(
                &mut record.bytes[MESH_LOD_INDEX..],
                i32::try_from(*bone_index)?,
            );
        }
        records.push(record);
    }
    Ok(records)
}

fn build_record_table(
    records: &[RawRecord],
    trailer: Option<u32>,
    absolute_offset: usize,
) -> crate::Result<Vec<u8>> {
    let count = records.len();
    let header_size = 4 + count * 8 + trailer.map(|_| 4).unwrap_or(0);
    let records_start = align_up(absolute_offset + header_size, 16) - absolute_offset;
    let mut output = vec![0u8; records_start];
    LE::write_u32(&mut output[0..], u32::try_from(count)?);
    let mut cursor = records_start;
    for (index, record) in records.iter().enumerate() {
        LE::write_u32(&mut output[4 + index * 4..], u32::try_from(cursor)?);
        LE::write_u32(&mut output[4 + count * 4 + index * 4..], record.marker);
        output.extend_from_slice(&record.bytes);
        cursor += record.bytes.len();
    }
    if let Some(value) = trailer {
        LE::write_u32(&mut output[4 + count * 8..], value);
    }
    Ok(output)
}

fn splice_unit_tables(
    source: &[u8],
    unit: &ParsedUnit,
    stream_table: &[u8],
    mesh_table: &[u8],
) -> crate::Result<Vec<u8>> {
    let replacement_len = stream_table.len() + mesh_table.len();
    let old_len = unit.materials_offset - unit.stream_offset;
    let delta = replacement_len as i64 - old_len as i64;
    let mut output = Vec::with_capacity((source.len() as i64 + delta) as usize);
    output.extend_from_slice(&source[..unit.stream_offset]);
    output.extend_from_slice(stream_table);
    output.extend_from_slice(mesh_table);
    output.extend_from_slice(&source[unit.materials_offset..]);
    update_top_level_offsets(&mut output, unit, stream_table.len(), delta)?;
    write_ending_mesh_count(&mut output, mesh_table)?;
    Ok(output)
}

fn update_top_level_offsets(
    output: &mut [u8],
    unit: &ParsedUnit,
    stream_table_len: usize,
    delta: i64,
) -> crate::Result<()> {
    for field in (TOP_LEVEL_OFFSET_START..=TOP_LEVEL_OFFSET_END).step_by(4) {
        let value = read_u32(output, field, "Unit top-level offset")? as usize;
        if value >= unit.materials_offset {
            write_adjusted_offset(output, field, value, delta)?;
        }
    }
    LE::write_u32(
        &mut output[STREAM_INFO_OFFSET_FIELD..],
        u32::try_from(unit.stream_offset)?,
    );
    LE::write_u32(
        &mut output[MESH_INFO_OFFSET_FIELD..],
        u32::try_from(unit.stream_offset + stream_table_len)?,
    );
    Ok(())
}

fn write_adjusted_offset(
    output: &mut [u8],
    field: usize,
    value: usize,
    delta: i64,
) -> crate::Result<()> {
    let adjusted = i64::try_from(value)? + delta;
    LE::write_u32(&mut output[field..], u32::try_from(adjusted)?);
    Ok(())
}

fn write_ending_mesh_count(output: &mut [u8], mesh_table: &[u8]) -> crate::Result<()> {
    let ending = required_offset(output, ENDING_OFFSET_FIELD, "Unit ending")?;
    require_range(output, ending, 8, "Unit ending mesh count")?;
    let count = read_u32(mesh_table, 0, "rebuilt MeshInfo count")?;
    LE::write_u64(&mut output[ending..], u64::from(count));
    Ok(())
}

fn validate_replacement(
    output: &[u8],
    source: &ParsedUnit,
    target: &ParsedUnit,
) -> crate::Result<()> {
    let parsed = parse_unit(output)?;
    let expected_culling = target.meshes.iter().filter(|mesh| mesh.is_culling).count();
    let expected_visible = source.meshes.iter().filter(|mesh| !mesh.is_culling).count();
    let actual_culling = parsed.meshes.iter().filter(|mesh| mesh.is_culling).count();
    if actual_culling != expected_culling
        || parsed.meshes.len() != expected_visible + expected_culling
    {
        eyre::bail!("rebuilt Unit failed culling-set validation");
    }
    Ok(())
}

fn required_offset(unit: &[u8], field: usize, label: &str) -> crate::Result<usize> {
    let offset = read_u32(unit, field, label)? as usize;
    if offset == 0 {
        eyre::bail!("Unit has no {label} table");
    }
    Ok(offset)
}

fn read_u32(data: &[u8], offset: usize, label: &str) -> crate::Result<u32> {
    require_range(data, offset, 4, label)?;
    Ok(LE::read_u32(&data[offset..]))
}

fn read_i32(data: &[u8], offset: usize, label: &str) -> crate::Result<i32> {
    require_range(data, offset, 4, label)?;
    Ok(LE::read_i32(&data[offset..]))
}

fn require_range(data: &[u8], start: usize, len: usize, label: &str) -> crate::Result<()> {
    let end = start
        .checked_add(len)
        .ok_or_else(|| eyre::eyre!("{label} range overflow"))?;
    if end > data.len() {
        eyre::bail!("{label} is out of bounds: {start}..{end} > {}", data.len());
    }
    Ok(())
}

fn align_up(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) / alignment * alignment
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::toc_only::TocOnlyPackage;
    use crate::constants::UNIT_ID;
    use std::path::Path;

    #[test]
    fn default_policy_keeps_patch_culling() {
        assert_eq!(CullingPolicy::default(), CullingPolicy::Patch);
        let decoded: CullingPolicy = serde_json::from_str("\"patch\"").unwrap();
        assert_eq!(decoded, CullingPolicy::Patch);
    }

    #[test]
    fn zero_section_mesh_is_not_culling() {
        let mesh = mesh_record(&[7], &[]);
        let parsed = parse_mesh(
            RawRecord {
                marker: 1,
                bytes: mesh,
            },
            &HashSet::new(),
        )
        .unwrap();
        assert!(!parsed.is_culling);
    }

    #[test]
    fn default_material_sections_are_culling() {
        let mesh = mesh_record(&[7], &[0]);
        let parsed = parse_mesh(
            RawRecord {
                marker: 1,
                bytes: mesh,
            },
            &HashSet::from([9]),
        )
        .unwrap();
        assert!(parsed.is_culling);
    }

    #[test]
    fn known_material_section_is_visible() {
        let mesh = mesh_record(&[7], &[0]);
        let parsed = parse_mesh(
            RawRecord {
                marker: 1,
                bytes: mesh,
            },
            &HashSet::from([7]),
        )
        .unwrap();
        assert!(!parsed.is_culling);
    }

    #[test]
    fn mesh_with_any_known_material_slot_is_not_culling() {
        let mesh = mesh_record(&[7, 9], &[1]);
        let parsed = parse_mesh(
            RawRecord {
                marker: 1,
                bytes: mesh,
            },
            &HashSet::from([7]),
        )
        .unwrap();

        assert!(!parsed.is_culling);
    }

    #[test]
    fn real_fixtures_have_expected_culling_units() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_files");
        assert_fixture_count(&root.join("DP-8/9ba626afa44a3aa3.patch_0"), 22, 22);
        assert_fixture_count(&root.join("PH56&PH-9/9ba626afa44a3aa3.patch_0"), 26, 0);
        let dune = root.join(
            "SSD'S Stylized Dune 15086 0.1 2026-08-13T05-50Z IzUPRhJHc/9ba626afa44a3aa3.patch_0",
        );
        assert_fixture_count(&dune, 16, 13);
    }

    #[test]
    fn target_policy_replaces_the_complete_culling_set() {
        let patch = synthetic_entry(&[false, true], &[1, 2, 3, 4, 5, 6, 7, 8]);
        let target = synthetic_entry(&[false, true, true], &[9, 10, 11, 12, 13, 14, 15, 16]);

        let output = replace_patch_culling_with_target(&patch, &target).unwrap();
        let inspection = inspect_unit_culling(&output.toc_data).unwrap();

        assert_eq!(inspection.mesh_count, 3);
        assert_eq!(inspection.culling_meshes.len(), 2);
        assert!(output.gpu_data.len() > patch.gpu_data.len());
        assert_eq!(parse_unit(&output.toc_data).unwrap().streams.len(), 2);
    }

    #[test]
    fn target_policy_removes_patch_culling_when_target_has_none() {
        let patch = synthetic_entry(&[false, true], &[1, 2, 3, 4, 5, 6, 7, 8]);
        let target = synthetic_entry(&[false], &[9, 10, 11, 12, 13, 14, 15, 16]);

        let output = replace_patch_culling_with_target(&patch, &target).unwrap();
        let inspection = inspect_unit_culling(&output.toc_data).unwrap();

        assert_eq!(inspection.mesh_count, 1);
        assert!(inspection.culling_meshes.is_empty());
    }

    #[test]
    fn target_policy_remaps_an_imported_transform_by_name_hash() {
        let patch = synthetic_entry(&[false, true], &[1, 2, 3, 4, 5, 6, 7, 8]);
        let mut target = synthetic_entry(&[false, true], &[9, 10, 11, 12, 13, 14, 15, 16]);
        let hash_offset = synthetic_transform_hash_offset();
        LE::write_u32(&mut target.toc_data[hash_offset..], 2);

        let output = replace_patch_culling_with_target(&patch, &target).unwrap();
        let inspection = inspect_unit_culling(&output.toc_data).unwrap();

        assert_eq!(inspection.culling_meshes[0].transform_index, 1);
    }

    #[test]
    fn target_policy_imports_and_remaps_bone_info() {
        let patch = synthetic_entry(&[false, true], &[1, 2, 3, 4, 5, 6, 7, 8]);
        let target = synthetic_entry_with_lod(&[false, true], &[9, 10, 11, 12, 13, 14, 15, 16], 0);

        let output = replace_patch_culling_with_target(&patch, &target).unwrap();
        let inspection = inspect_unit_culling(&output.toc_data).unwrap();

        assert_eq!(inspection.culling_meshes[0].lod_index, 1);
    }

    #[test]
    fn malformed_mesh_offset_is_a_parse_error() {
        let mut entry = synthetic_entry(&[false], &[1, 2, 3, 4, 5, 6, 7, 8]);
        LE::write_u32(&mut entry.toc_data[MESH_INFO_OFFSET_FIELD..], u32::MAX);

        assert!(inspect_unit_culling(&entry.toc_data).is_err());
    }

    #[test]
    fn unsafe_target_dependency_aborts_replacement() {
        let patch = synthetic_entry(&[false, true], &[1, 2, 3, 4, 5, 6, 7, 8]);
        let mut target = synthetic_entry(&[false, true], &[9, 10, 11, 12, 13, 14, 15, 16]);
        set_mesh_transform(&mut target.toc_data, 1, 99);

        assert!(replace_patch_culling_with_target(&patch, &target).is_err());
    }

    fn assert_fixture_count(path: &Path, expected_units: usize, expected_culling: usize) {
        if !path.exists() {
            return;
        }
        let bytes = std::fs::read(path).expect("read culling fixture");
        let package = TocOnlyPackage::parse(&bytes).expect("parse culling fixture");
        let units = package
            .entries
            .iter()
            .filter(|entry| entry.type_id == UNIT_ID)
            .collect::<Vec<_>>();
        let culling = units
            .iter()
            .filter(|entry| {
                inspect_unit_culling(&entry.toc_data)
                    .expect("inspect fixture Unit")
                    .culling_meshes
                    .len()
                    > 0
            })
            .count();
        assert_eq!(units.len(), expected_units);
        assert_eq!(culling, expected_culling);
    }

    fn synthetic_entry(culling: &[bool], gpu: &[u8]) -> TocEntry {
        synthetic_entry_with_lod(culling, gpu, -1)
    }

    fn synthetic_entry_with_lod(culling: &[bool], gpu: &[u8], culling_lod: i32) -> TocEntry {
        let transform_offset = 0x80usize;
        let bone_offset = 0x120usize;
        let stream_offset = 0x140usize;
        let stream = synthetic_stream_record();
        let stream_table = build_record_table(&[stream], Some(0), stream_offset).unwrap();
        let mesh_offset = stream_offset + stream_table.len();
        let meshes = culling
            .iter()
            .enumerate()
            .map(|(index, is_culling)| {
                let mut bytes = mesh_record(&[if *is_culling { 9 } else { 7 }], &[0]);
                if *is_culling {
                    LE::write_i32(&mut bytes[MESH_LOD_INDEX..], culling_lod);
                }
                RawRecord {
                    marker: index as u32,
                    bytes,
                }
            })
            .collect::<Vec<_>>();
        let mesh_table = build_record_table(&meshes, None, mesh_offset).unwrap();
        let materials_offset = mesh_offset + mesh_table.len();
        let ending_offset = materials_offset + 16;
        let mut toc = vec![0u8; ending_offset + 8];
        LE::write_u32(&mut toc[TRANSFORM_OFFSET_FIELD..], transform_offset as u32);
        LE::write_u32(&mut toc[BONE_INFO_OFFSET_FIELD..], bone_offset as u32);
        LE::write_u32(&mut toc[STREAM_INFO_OFFSET_FIELD..], stream_offset as u32);
        LE::write_u32(&mut toc[MESH_INFO_OFFSET_FIELD..], mesh_offset as u32);
        LE::write_u32(&mut toc[MATERIALS_OFFSET_FIELD..], materials_offset as u32);
        LE::write_u32(&mut toc[ENDING_OFFSET_FIELD..], ending_offset as u32);
        LE::write_u32(&mut toc[transform_offset..], 1);
        LE::write_u32(&mut toc[synthetic_transform_hash_offset()..], 1);
        write_synthetic_bone_table(&mut toc, bone_offset);
        toc[stream_offset..mesh_offset].copy_from_slice(&stream_table);
        toc[mesh_offset..materials_offset].copy_from_slice(&mesh_table);
        LE::write_u32(&mut toc[materials_offset..], 1);
        LE::write_u32(&mut toc[materials_offset + 4..], 7);
        LE::write_u64(&mut toc[materials_offset + 8..], 77);
        LE::write_u64(&mut toc[ending_offset..], culling.len() as u64);
        let mut entry = TocEntry::new(1, UNIT_ID);
        entry.toc_data = toc;
        entry.gpu_data = gpu.to_vec().into();
        entry
    }

    fn synthetic_transform_hash_offset() -> usize {
        0x80 + TRANSFORM_HEADER_SIZE
            + LOCAL_TRANSFORM_SIZE
            + TRANSFORM_MATRIX_SIZE
            + TRANSFORM_ENTRY_SIZE
    }

    fn write_synthetic_bone_table(toc: &mut [u8], offset: usize) {
        LE::write_u32(&mut toc[offset..], 1);
        LE::write_u32(&mut toc[offset + 4..], 8);
        let record = offset + 8;
        LE::write_u32(&mut toc[record..], 1);
        LE::write_u32(&mut toc[record + 8..], 16);
        LE::write_u32(&mut toc[record + 16..], 0);
    }

    fn set_mesh_transform(toc: &mut [u8], mesh_index: usize, transform_index: u32) {
        let table = LE::read_u32(&toc[MESH_INFO_OFFSET_FIELD..]) as usize;
        let relative = LE::read_u32(&toc[table + 4 + mesh_index * 4..]) as usize;
        LE::write_u32(
            &mut toc[table + relative + MESH_TRANSFORM_INDEX..],
            transform_index,
        );
    }

    fn synthetic_stream_record() -> RawRecord {
        let mut bytes = vec![0u8; 448];
        LE::write_u32(&mut bytes[STREAM_VERTEX_OFFSET..], 0);
        LE::write_u32(&mut bytes[STREAM_VERTEX_SIZE..], 4);
        LE::write_u32(&mut bytes[STREAM_INDEX_OFFSET..], 4);
        LE::write_u32(&mut bytes[STREAM_INDEX_SIZE..], 4);
        RawRecord { marker: 0, bytes }
    }

    fn mesh_record(materials: &[u32], section_materials: &[u32]) -> Vec<u8> {
        let material_offset = 128usize;
        let sections_offset = material_offset + materials.len() * 4;
        let mut bytes = vec![0u8; sections_offset + section_materials.len() * MESH_SECTION_SIZE];
        LE::write_u32(&mut bytes[MESH_NUM_MATERIALS..], materials.len() as u32);
        LE::write_u32(&mut bytes[MESH_MATERIAL_OFFSET..], material_offset as u32);
        LE::write_u32(
            &mut bytes[MESH_NUM_SECTIONS..],
            section_materials.len() as u32,
        );
        LE::write_u32(&mut bytes[MESH_SECTIONS_OFFSET..], sections_offset as u32);
        LE::write_i32(&mut bytes[MESH_LOD_INDEX..], -1);
        for (index, material) in materials.iter().enumerate() {
            LE::write_u32(&mut bytes[material_offset + index * 4..], *material);
        }
        for (index, material) in section_materials.iter().enumerate() {
            LE::write_u32(
                &mut bytes[sections_offset + index * MESH_SECTION_SIZE..],
                *material,
            );
        }
        bytes
    }
}
