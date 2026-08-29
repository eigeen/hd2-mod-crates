use super::vertex::{
    CURRENT_VERSION, Component, LEGACY_VERSION, Semantic, component_size, decode, default_value,
    encode, float_values, relative_transform, transform_direction, transform_position, uint_values,
    validate_component,
};
use crate::archive::TocEntry;
use crate::hashing::{murmur32, murmur64};
use byteorder::{ByteOrder, LittleEndian as LE};
use std::collections::{HashMap, HashSet};

const TRANSFORM_OFFSET_FIELD: usize = 0x34;
const BONE_OFFSET_FIELD: usize = 0x58;
const STREAM_OFFSET_FIELD: usize = 0x5c;
const ENDING_OFFSET_FIELD: usize = 0x60;
const MESH_OFFSET_FIELD: usize = 0x64;
const MATERIAL_OFFSET_FIELD: usize = 0x70;
const VERSION_FIELD: usize = 0x2c;
const STREAM_MIN_SIZE: usize = 432;
const MESH_MIN_SIZE: usize = 128;
const SECTION_SIZE: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeDiagnostic {
    pub lod: i32,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct MergeOutcome {
    pub entry: TocEntry,
    pub merged_lods: Vec<i32>,
    pub skipped_lods: Vec<MergeDiagnostic>,
}

pub(super) fn visible_material_file_ids(entry: &TocEntry) -> crate::Result<Vec<u64>> {
    let unit = ParsedUnit::parse(entry)?;
    let slots = unit.normal_slots();
    let mut ids = Vec::new();
    for (mesh_index, mesh) in unit.meshes.iter().enumerate() {
        if mesh.lod < 0 || mesh.visible_vertex_count(&slots) == 0 {
            continue;
        }
        for section_index in 0..mesh.sections.len() {
            if let Ok((slot, material_id)) = unit.section_material(mesh_index, section_index)
                && slots.contains(&slot)
            {
                ids.push(material_id);
            }
        }
    }
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

#[derive(Debug, Clone)]
struct RawRecord {
    marker: u32,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
struct StreamRecord {
    raw: RawRecord,
    components: Vec<Component>,
    vertex_count: u32,
    vertex_stride: u32,
    index_count: u32,
    index_type: u32,
    vertex_offset: u32,
    vertex_size: u32,
    index_offset: u32,
    index_size: u32,
}

#[derive(Debug, Clone, Copy)]
struct MeshSection {
    material_index: u32,
    vertex_offset: u32,
    vertex_count: u32,
    index_offset: u32,
    index_count: u32,
    group_index: u32,
}

#[derive(Debug, Clone)]
struct MeshRecord {
    raw: RawRecord,
    dirty: bool,
    transform_index: usize,
    lod: i32,
    stream_index: usize,
    material_slots: Vec<u32>,
    sections: Vec<MeshSection>,
}

#[derive(Debug, Clone)]
struct TransformRecord {
    matrix: [f32; 16],
    name_hash: u32,
}

#[derive(Debug, Clone)]
struct BoneRecord {
    raw: Vec<u8>,
    dirty: bool,
    real_indices: Vec<usize>,
    remaps: Vec<Vec<u32>>,
}

#[derive(Debug, Clone)]
struct ParsedUnit {
    toc: Vec<u8>,
    gpu: Vec<u8>,
    version: u32,
    stream_unknown: u32,
    streams: Vec<StreamRecord>,
    meshes: Vec<MeshRecord>,
    transforms: Vec<TransformRecord>,
    bones: Vec<BoneRecord>,
    materials: Vec<(u32, u64)>,
    material_layout: MaterialTableLayout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MaterialTableLayout {
    /// One slot-to-material mapping is shared by every mesh.
    Global,
    /// Exporters repeat mappings in the same order as each mesh's slots.
    PerMesh,
}

pub fn merge_visible_head_into_body(
    body: &TocEntry,
    helmet: &TocEntry,
    material_file_ids: &HashMap<u64, u64>,
) -> crate::Result<MergeOutcome> {
    let mut output = ParsedUnit::parse(body)?;
    let source = ParsedUnit::parse(helmet)?;
    let lods = output.visible_lods();
    let mut merged_lods = Vec::new();
    let mut skipped_lods = Vec::new();

    for lod in lods {
        let mut candidate = output.clone();
        match candidate.merge_lod(&source, lod, material_file_ids) {
            Ok(()) => {
                output = candidate;
                merged_lods.push(lod);
            }
            Err(error) => skipped_lods.push(MergeDiagnostic {
                lod,
                reason: error.to_string(),
            }),
        }
    }
    if merged_lods.is_empty() {
        let reasons = skipped_lods
            .iter()
            .map(|item| format!("LOD {}: {}", item.lod, item.reason))
            .collect::<Vec<_>>()
            .join("; ");
        if reasons.is_empty() {
            eyre::bail!("body Unit has no visible LOD");
        }
        eyre::bail!("no compatible visible LOD was found: {reasons}");
    }

    let mut entry = body.clone();
    entry.toc_data = output.rebuild_toc()?;
    entry.gpu_data = output.gpu.into();
    ParsedUnit::parse(&entry)
        .map_err(|error| eyre::eyre!("rebuilt Unit validation failed: {error}"))?;
    Ok(MergeOutcome {
        entry,
        merged_lods,
        skipped_lods,
    })
}

impl ParsedUnit {
    fn parse(entry: &TocEntry) -> crate::Result<Self> {
        let toc = entry.toc_data.to_vec();
        require_range(&toc, VERSION_FIELD, 4, "Unit version")?;
        let version = LE::read_u32(&toc[VERSION_FIELD..]);
        if !matches!(version, LEGACY_VERSION | CURRENT_VERSION) {
            eyre::bail!("unsupported Unit version {version}");
        }
        let bone_offset = required_offset(&toc, BONE_OFFSET_FIELD, "BoneInfo")?;
        let stream_offset = required_offset(&toc, STREAM_OFFSET_FIELD, "StreamInfo")?;
        let mesh_offset = required_offset(&toc, MESH_OFFSET_FIELD, "MeshInfo")?;
        let material_offset = required_offset(&toc, MATERIAL_OFFSET_FIELD, "materials")?;
        if !(bone_offset < stream_offset
            && stream_offset < mesh_offset
            && mesh_offset < material_offset)
        {
            eyre::bail!("unsupported Unit table order");
        }
        let (stream_records, stream_unknown) =
            read_record_table(&toc, stream_offset, mesh_offset, STREAM_MIN_SIZE, true)?;
        let (mesh_records, _) =
            read_record_table(&toc, mesh_offset, material_offset, MESH_MIN_SIZE, false)?;
        let streams = stream_records
            .into_iter()
            .map(|record| StreamRecord::parse(record, version))
            .collect::<crate::Result<Vec<_>>>()?;
        let meshes = mesh_records
            .into_iter()
            .map(MeshRecord::parse)
            .collect::<crate::Result<Vec<_>>>()?;
        for mesh in &meshes {
            if mesh.stream_index >= streams.len() {
                eyre::bail!("MeshInfo references missing stream {}", mesh.stream_index);
            }
        }
        let materials = parse_materials(&toc, material_offset)?;
        let material_layout = detect_material_table_layout(&meshes, &materials);
        let parsed = Self {
            transforms: parse_transforms(&toc)?,
            bones: parse_bones(&toc, bone_offset, stream_offset)?,
            materials,
            material_layout,
            toc,
            gpu: entry.gpu_data.to_vec(),
            version,
            stream_unknown,
            streams,
            meshes,
        };
        parsed.validate_ranges()?;
        Ok(parsed)
    }

    fn visible_lods(&self) -> Vec<i32> {
        let slots = self.normal_slots();
        let mut lods = self
            .meshes
            .iter()
            .filter(|mesh| mesh.lod >= 0 && mesh.visible_vertex_count(&slots) > 0)
            .map(|mesh| mesh.lod)
            .collect::<Vec<_>>();
        lods.sort_unstable();
        lods.dedup();
        lods
    }

    fn normal_slots(&self) -> HashSet<u32> {
        self.materials.iter().map(|(slot, _)| *slot).collect()
    }

    fn merge_lod(
        &mut self,
        source: &ParsedUnit,
        target_lod: i32,
        material_file_ids: &HashMap<u64, u64>,
    ) -> crate::Result<()> {
        let mut failures = Vec::new();
        for source_lod in source_lods_by_distance(source, target_lod) {
            let mut candidate = self.clone();
            let lods = LodPair {
                target: target_lod,
                source: source_lod,
            };
            match candidate.merge_lod_pair(source, lods, material_file_ids) {
                Ok(()) => {
                    *self = candidate;
                    return Ok(());
                }
                Err(error) => failures.push(format!("helmet LOD {source_lod}: {error}")),
            }
        }
        if failures.is_empty() {
            eyre::bail!("helmet has no visible LOD");
        }
        eyre::bail!(failures.join("; "))
    }

    fn merge_lod_pair(
        &mut self,
        source: &ParsedUnit,
        lods: LodPair,
        material_file_ids: &HashMap<u64, u64>,
    ) -> crate::Result<()> {
        let target_slots = self.normal_slots();
        let source_slots = source.normal_slots();
        let host_index = select_host_mesh(&self.meshes, lods.target, &target_slots)?;
        let source_sections = visible_source_sections(source, lods.source, &source_slots);
        if source_sections.is_empty() {
            eyre::bail!("helmet has no visible mesh");
        }
        for source_section in source_sections {
            self.append_source_section(source, host_index, source_section, material_file_ids)?;
        }
        Ok(())
    }

    fn append_source_section(
        &mut self,
        source: &ParsedUnit,
        host_index: usize,
        source_section: SourceSection,
        material_file_ids: &HashMap<u64, u64>,
    ) -> crate::Result<()> {
        let source_mesh = &source.meshes[source_section.mesh_index];
        let source_stream = &source.streams[source_mesh.stream_index];
        let host_stream_index = self.meshes[host_index].stream_index;
        let host_transform = self.transform_matrix(self.meshes[host_index].transform_index)?;
        let source_transform = source.transform_matrix(source_mesh.transform_index)?;
        let transform = relative_transform(&source_transform, &host_transform)?;
        validate_layout_compatibility(
            &self.streams[host_stream_index],
            source_stream,
            self.version,
            source.version,
        )?;

        let (slot_id, material_id) =
            source.section_material(source_section.mesh_index, source_section.section_index)?;
        let mapped_material = material_file_ids
            .get(&material_id)
            .copied()
            .unwrap_or(material_id);
        let local_material_index = self.add_mesh_material(host_index, slot_id, mapped_material)?;
        let group_index = self.append_bone_remap(source, host_index, source_section)?;
        let source_section_value = source_mesh.sections[source_section.section_index];
        let request = StreamAppendRequest {
            source_gpu: &source.gpu,
            source: source_stream,
            source_version: source.version,
            section: source_section_value,
            transform: &transform,
        };
        let appended = append_stream_data(
            &mut self.gpu,
            &mut self.streams[host_stream_index],
            self.version,
            request,
        )?;
        self.meshes[host_index].dirty = true;
        self.meshes[host_index].sections.push(MeshSection {
            material_index: local_material_index,
            vertex_offset: appended.vertex_offset,
            vertex_count: appended.vertex_count,
            index_offset: appended.index_offset,
            index_count: appended.index_count,
            group_index,
        });
        Ok(())
    }

    fn append_bone_remap(
        &mut self,
        source: &ParsedUnit,
        host_index: usize,
        source_section: SourceSection,
    ) -> crate::Result<u32> {
        let source_mesh = &source.meshes[source_section.mesh_index];
        let source_stream = &source.streams[source_mesh.stream_index];
        if !source_stream
            .components
            .iter()
            .any(|component| component.semantic.kind == 6)
        {
            return Ok(0);
        }
        let target_lod = self.meshes[host_index].lod;
        if source_mesh.lod < 0 || target_lod < 0 {
            return Ok(0);
        }
        let source_bone = source
            .bones
            .get(source_mesh.lod as usize)
            .ok_or_else(|| eyre::eyre!("helmet LOD {} has no BoneInfo", source_mesh.lod))?;
        let source_group = source_mesh.sections[source_section.section_index].group_index as usize;
        let source_remap = source_bone.remaps.get(source_group).ok_or_else(|| {
            eyre::eyre!("helmet section references missing bone group {source_group}")
        })?;
        let used_slots = section_used_bone_slots(source, source_stream, source_section)?;
        if used_slots.iter().any(|slot| *slot >= source_remap.len()) {
            eyre::bail!("helmet vertex references a bone outside section remap");
        }
        let target_bone = self
            .bones
            .get_mut(target_lod as usize)
            .ok_or_else(|| eyre::eyre!("body LOD {target_lod} has no BoneInfo"))?;
        let mut remap = Vec::with_capacity(source_remap.len());
        for (remap_slot, source_real_slot) in source_remap.iter().enumerate() {
            if !used_slots.contains(&remap_slot) {
                remap.push(0);
                continue;
            }
            let source_transform_index = *source_bone
                .real_indices
                .get(*source_real_slot as usize)
                .ok_or_else(|| eyre::eyre!("helmet bone remap index is out of bounds"))?;
            let hash = source
                .transforms
                .get(source_transform_index)
                .ok_or_else(|| eyre::eyre!("helmet BoneInfo references missing transform"))?
                .name_hash;
            let target_transform_index = self
                .transforms
                .iter()
                .position(|record| record.name_hash == hash)
                .ok_or_else(|| {
                    eyre::eyre!("body skeleton is missing helmet bone hash 0x{hash:08x}")
                })?;
            let target_real_slot = target_bone
                .real_indices
                .iter()
                .position(|index| *index == target_transform_index)
                .ok_or_else(|| {
                    eyre::eyre!("body LOD BoneInfo does not contain helmet bone 0x{hash:08x}")
                })?;
            remap.push(u32::try_from(target_real_slot)?);
        }
        let index = target_bone.remaps.len();
        target_bone.remaps.push(remap);
        target_bone.dirty = true;
        Ok(u32::try_from(index)?)
    }

    fn transform_matrix(&self, index: usize) -> crate::Result<[f32; 16]> {
        self.transforms
            .get(index)
            .map(|record| record.matrix)
            .ok_or_else(|| eyre::eyre!("mesh references missing transform {index}"))
    }

    fn section_material(
        &self,
        mesh_index: usize,
        section_index: usize,
    ) -> crate::Result<(u32, u64)> {
        let mesh = self
            .meshes
            .get(mesh_index)
            .ok_or_else(|| eyre::eyre!("missing mesh {mesh_index}"))?;
        let section = mesh.sections[section_index];
        let local_index = section.material_index as usize;
        let slot = *mesh
            .material_slots
            .get(local_index)
            .ok_or_else(|| eyre::eyre!("section material index is out of bounds"))?;
        let material = self.mesh_material(mesh_index, local_index)?;
        if material.0 != slot {
            eyre::bail!("mesh slot 0x{slot:08x} does not match its material table entry");
        }
        Ok(material)
    }

    fn add_mesh_material(
        &mut self,
        mesh_index: usize,
        requested_slot: u32,
        material_id: u64,
    ) -> crate::Result<u32> {
        if let Some(local_index) = self.meshes[mesh_index]
            .material_slots
            .iter()
            .position(|slot| *slot == requested_slot)
            && self.section_material_id(mesh_index, local_index)? == material_id
        {
            return Ok(u32::try_from(local_index)?);
        }
        let output_slot = if self
            .materials
            .iter()
            .any(|(slot, id)| *slot == requested_slot && *id != material_id)
        {
            unique_slot(&self.materials, requested_slot, material_id)
        } else {
            requested_slot
        };
        let material_index = self.mesh_material_end(mesh_index)?;
        let local_index = self.meshes[mesh_index].append_material_slot(output_slot)?;
        match self.material_layout {
            MaterialTableLayout::PerMesh => {
                self.materials
                    .insert(material_index, (output_slot, material_id));
            }
            MaterialTableLayout::Global => {
                if !self.materials.contains(&(output_slot, material_id)) {
                    self.materials.push((output_slot, material_id));
                }
            }
        }
        Ok(local_index)
    }

    fn section_material_id(&self, mesh_index: usize, local_index: usize) -> crate::Result<u64> {
        self.mesh_material(mesh_index, local_index)
            .map(|(_, material_id)| material_id)
    }

    fn mesh_material(&self, mesh_index: usize, local_index: usize) -> crate::Result<(u32, u64)> {
        let slot = self.meshes[mesh_index].material_slots[local_index];
        match self.material_layout {
            MaterialTableLayout::PerMesh => {
                let index = self.mesh_material_start(mesh_index)? + local_index;
                self.materials
                    .get(index)
                    .copied()
                    .ok_or_else(|| eyre::eyre!("mesh material table is truncated"))
            }
            MaterialTableLayout::Global => self
                .materials
                .iter()
                .find(|(candidate, _)| *candidate == slot)
                .copied()
                .ok_or_else(|| eyre::eyre!("section slot 0x{slot:08x} has no material")),
        }
    }

    fn mesh_material_start(&self, mesh_index: usize) -> crate::Result<usize> {
        if mesh_index >= self.meshes.len() {
            eyre::bail!("missing mesh {mesh_index}");
        }
        Ok(self.meshes[..mesh_index]
            .iter()
            .map(|mesh| mesh.material_slots.len())
            .sum())
    }

    fn mesh_material_end(&self, mesh_index: usize) -> crate::Result<usize> {
        Ok(self.mesh_material_start(mesh_index)? + self.meshes[mesh_index].material_slots.len())
    }

    fn rebuild_toc(&self) -> crate::Result<Vec<u8>> {
        self.validate_material_table()?;
        let mut output = self.toc.clone();
        if self.bones.iter().any(|record| record.dirty) {
            rebuild_changed_bones(&mut output, &self.bones)?;
        }

        let stream_start = required_offset(&output, STREAM_OFFSET_FIELD, "StreamInfo")?;
        let mesh_start = required_offset(&output, MESH_OFFSET_FIELD, "MeshInfo")?;
        let streams = self
            .streams
            .iter()
            .map(StreamRecord::build_raw)
            .collect::<crate::Result<Vec<_>>>()?;
        let stream_table = build_record_table(&streams, Some(self.stream_unknown), stream_start)?;
        replace_range_and_offsets(&mut output, stream_start, mesh_start, &stream_table)?;

        let mesh_start = required_offset(&output, MESH_OFFSET_FIELD, "MeshInfo")?;
        let material_start = required_offset(&output, MATERIAL_OFFSET_FIELD, "materials")?;
        let meshes = self
            .meshes
            .iter()
            .map(MeshRecord::build_raw)
            .collect::<crate::Result<Vec<_>>>()?;
        let mesh_table = build_record_table(&meshes, None, mesh_start)?;
        replace_range_and_offsets(&mut output, mesh_start, material_start, &mesh_table)?;

        let material_start = required_offset(&output, MATERIAL_OFFSET_FIELD, "materials")?;
        let old_material_end = material_table_end(&output, material_start)?;
        let material_table = build_material_table(&self.materials)?;
        replace_range_and_offsets(
            &mut output,
            material_start,
            old_material_end,
            &material_table,
        )?;
        let ending = required_offset(&output, ENDING_OFFSET_FIELD, "Unit ending")?;
        require_range(&output, ending, 8, "Unit ending")?;
        LE::write_u64(&mut output[ending..], self.meshes.len() as u64);
        Ok(output)
    }

    fn validate_ranges(&self) -> crate::Result<()> {
        for stream in &self.streams {
            validate_stream_range(stream, &self.gpu)?;
        }
        for mesh in &self.meshes {
            validate_mesh_sections(self, mesh)?;
        }
        for bone in &self.bones {
            validate_bone_record(bone, self.transforms.len())?;
        }
        self.validate_material_table()?;
        Ok(())
    }

    fn validate_material_table(&self) -> crate::Result<()> {
        if self.material_layout == MaterialTableLayout::Global {
            return Ok(());
        }
        let expected = self
            .meshes
            .iter()
            .map(|mesh| mesh.material_slots.len())
            .sum::<usize>();
        if self.materials.len() != expected {
            eyre::bail!(
                "Unit material table has {} entries for {expected} mesh slots",
                self.materials.len()
            );
        }
        for (mesh_index, mesh) in self.meshes.iter().enumerate() {
            self.validate_mesh_material_slots(mesh_index, mesh)?;
        }
        Ok(())
    }

    fn validate_mesh_material_slots(
        &self,
        mesh_index: usize,
        mesh: &MeshRecord,
    ) -> crate::Result<()> {
        let start = self.mesh_material_start(mesh_index)?;
        for (local_index, slot) in mesh.material_slots.iter().enumerate() {
            if self.materials[start + local_index].0 != *slot {
                eyre::bail!("mesh material slots do not match the Unit material table");
            }
        }
        Ok(())
    }
}

/// Detect whether material mappings are shared or repeated beside every mesh.
fn detect_material_table_layout(
    meshes: &[MeshRecord],
    materials: &[(u32, u64)],
) -> MaterialTableLayout {
    let slots = meshes
        .iter()
        .flat_map(|mesh| mesh.material_slots.iter().copied())
        .collect::<Vec<_>>();
    let aligned = slots
        .iter()
        .zip(materials)
        .all(|(slot, (material_slot, _))| slot == material_slot);
    if slots.len() == materials.len() && aligned {
        MaterialTableLayout::PerMesh
    } else {
        MaterialTableLayout::Global
    }
}

#[derive(Debug, Clone, Copy)]
struct LodPair {
    target: i32,
    source: i32,
}

fn source_lods_by_distance(source: &ParsedUnit, target_lod: i32) -> Vec<i32> {
    let mut lods = source.visible_lods();
    lods.sort_by_key(|source_lod| (source_lod.abs_diff(target_lod), *source_lod));
    lods
}

fn rebuild_changed_bones(output: &mut Vec<u8>, bones: &[BoneRecord]) -> crate::Result<()> {
    let bone_start = required_offset(output, BONE_OFFSET_FIELD, "BoneInfo")?;
    let stream_start = required_offset(output, STREAM_OFFSET_FIELD, "StreamInfo")?;
    let bone_table = build_bone_table(bone_start, bones)?;
    replace_range_and_offsets(output, bone_start, stream_start, &bone_table)
}

#[derive(Debug, Clone, Copy)]
struct SourceSection {
    mesh_index: usize,
    section_index: usize,
}

#[derive(Debug, Clone, Copy)]
struct AppendedSection {
    vertex_offset: u32,
    vertex_count: u32,
    index_offset: u32,
    index_count: u32,
}

struct StreamAppendRequest<'a> {
    source_gpu: &'a [u8],
    source: &'a StreamRecord,
    source_version: u32,
    section: MeshSection,
    transform: &'a [f32; 16],
}

impl StreamRecord {
    fn parse(raw: RawRecord, version: u32) -> crate::Result<Self> {
        require_range(&raw.bytes, 0, STREAM_MIN_SIZE, "StreamInfo record")?;
        let count = read_u64(&raw.bytes, 328, "component count")? as usize;
        if count > 16 {
            eyre::bail!("StreamInfo has {count} components; maximum is 16");
        }
        let mut cursor = 0usize;
        let mut components = Vec::with_capacity(count);
        for index in 0..count {
            let offset = 8 + index * 20;
            let component = Component {
                semantic: Semantic {
                    kind: read_u32(&raw.bytes, offset, "component type")?,
                    index: read_u32(&raw.bytes, offset + 8, "component index")?,
                },
                format: read_u32(&raw.bytes, offset + 4, "component format")?,
                offset: cursor,
            };
            validate_component(version, component)?;
            cursor += component_size(version, component.format).unwrap_or(0);
            components.push(component);
        }
        let vertex_stride = read_u32(&raw.bytes, 356, "vertex stride")?;
        if cursor > vertex_stride as usize {
            eyre::bail!("vertex components exceed declared stride");
        }
        Ok(Self {
            vertex_count: read_u32(&raw.bytes, 352, "vertex count")?,
            vertex_stride,
            index_count: read_u32(&raw.bytes, 392, "index count")?,
            index_type: read_u32(&raw.bytes, 396, "index type")?,
            vertex_offset: read_u32(&raw.bytes, 416, "vertex offset")?,
            vertex_size: read_u32(&raw.bytes, 420, "vertex size")?,
            index_offset: read_u32(&raw.bytes, 424, "index offset")?,
            index_size: read_u32(&raw.bytes, 428, "index size")?,
            components,
            raw,
        })
    }

    fn build_raw(&self) -> crate::Result<RawRecord> {
        let mut output = self.raw.clone();
        write_u32(&mut output.bytes, 352, self.vertex_count)?;
        write_u32(&mut output.bytes, 356, self.vertex_stride)?;
        write_u32(&mut output.bytes, 392, self.index_count)?;
        write_u32(&mut output.bytes, 396, self.index_type)?;
        write_u32(&mut output.bytes, 416, self.vertex_offset)?;
        write_u32(&mut output.bytes, 420, self.vertex_size)?;
        write_u32(&mut output.bytes, 424, self.index_offset)?;
        write_u32(&mut output.bytes, 428, self.index_size)?;
        Ok(output)
    }
}

impl MeshRecord {
    fn parse(raw: RawRecord) -> crate::Result<Self> {
        require_range(&raw.bytes, 0, MESH_MIN_SIZE, "MeshInfo record")?;
        let material_count = read_u32(&raw.bytes, 104, "mesh material count")? as usize;
        let material_offset = read_u32(&raw.bytes, 108, "mesh material offset")? as usize;
        require_range(
            &raw.bytes,
            material_offset,
            material_count * 4,
            "mesh material slots",
        )?;
        let material_slots = (0..material_count)
            .map(|index| LE::read_u32(&raw.bytes[material_offset + index * 4..]))
            .collect();
        let section_count = read_u32(&raw.bytes, 120, "mesh section count")? as usize;
        let section_offset = read_u32(&raw.bytes, 124, "mesh section offset")? as usize;
        require_range(
            &raw.bytes,
            section_offset,
            section_count * SECTION_SIZE,
            "mesh sections",
        )?;
        let sections = (0..section_count)
            .map(|index| parse_section(&raw.bytes, section_offset + index * SECTION_SIZE))
            .collect::<crate::Result<Vec<_>>>()?;
        Ok(Self {
            transform_index: read_u32(&raw.bytes, 48, "mesh transform")? as usize,
            lod: read_i32(&raw.bytes, 56, "mesh LOD")?,
            stream_index: read_u32(&raw.bytes, 60, "mesh stream")? as usize,
            raw,
            dirty: false,
            material_slots,
            sections,
        })
    }

    fn visible_vertex_count(&self, normal_slots: &HashSet<u32>) -> u64 {
        self.sections
            .iter()
            .filter(|section| self.section_is_visible(section, normal_slots))
            .map(|section| u64::from(section.vertex_count))
            .sum()
    }

    fn section_is_visible(&self, section: &MeshSection, normal_slots: &HashSet<u32>) -> bool {
        self.material_slots
            .get(section.material_index as usize)
            .is_some_and(|slot| normal_slots.contains(slot))
    }

    fn append_material_slot(&mut self, slot: u32) -> crate::Result<u32> {
        let index = self.material_slots.len();
        self.material_slots.push(slot);
        self.dirty = true;
        Ok(u32::try_from(index)?)
    }

    fn build_raw(&self) -> crate::Result<RawRecord> {
        if !self.dirty {
            return Ok(self.raw.clone());
        }
        let mut output = self.raw.clone();
        let material_offset = align_up(output.bytes.len(), 4);
        output.bytes.resize(material_offset, 0);
        for slot in &self.material_slots {
            output.bytes.extend_from_slice(&slot.to_le_bytes());
        }
        let section_offset = align_up(output.bytes.len(), 4);
        output.bytes.resize(section_offset, 0);
        for section in &self.sections {
            write_section(&mut output.bytes, *section);
        }
        write_u32(
            &mut output.bytes,
            104,
            u32::try_from(self.material_slots.len())?,
        )?;
        write_u32(&mut output.bytes, 108, u32::try_from(material_offset)?)?;
        write_u32(&mut output.bytes, 120, u32::try_from(self.sections.len())?)?;
        write_u32(&mut output.bytes, 124, u32::try_from(section_offset)?)?;
        Ok(output)
    }
}

fn select_host_mesh(meshes: &[MeshRecord], lod: i32, slots: &HashSet<u32>) -> crate::Result<usize> {
    let candidates = meshes
        .iter()
        .enumerate()
        .filter(|(_, mesh)| mesh.lod == lod)
        .map(|(index, mesh)| (index, mesh.visible_vertex_count(slots)))
        .filter(|(_, count)| *count > 0)
        .collect::<Vec<_>>();
    let max = candidates
        .iter()
        .map(|(_, count)| *count)
        .max()
        .ok_or_else(|| eyre::eyre!("body has no visible mesh for LOD {lod}"))?;
    let winners = candidates
        .into_iter()
        .filter(|(_, count)| *count == max)
        .collect::<Vec<_>>();
    if winners.len() != 1 {
        eyre::bail!("body LOD {lod} has multiple equally-sized host meshes");
    }
    Ok(winners[0].0)
}

fn visible_source_sections(
    source: &ParsedUnit,
    lod: i32,
    slots: &HashSet<u32>,
) -> Vec<SourceSection> {
    source
        .meshes
        .iter()
        .enumerate()
        .filter(|(_, mesh)| mesh.lod == lod)
        .flat_map(|(mesh_index, mesh)| {
            mesh.sections
                .iter()
                .enumerate()
                .filter(|(_, section)| {
                    mesh.section_is_visible(section, slots) && section.index_count > 0
                })
                .map(move |(section_index, _)| SourceSection {
                    mesh_index,
                    section_index,
                })
        })
        .collect()
}

fn validate_layout_compatibility(
    target: &StreamRecord,
    source: &StreamRecord,
    target_version: u32,
    source_version: u32,
) -> crate::Result<()> {
    for component in &target.components {
        validate_component(target_version, *component)?;
    }
    for component in &source.components {
        validate_component(source_version, *component)?;
        if semantic_is_required_in_target(component.semantic)
            && !target
                .components
                .iter()
                .any(|target| target.semantic == component.semantic)
        {
            eyre::bail!(
                "body stream is missing required helmet vertex semantic {}/{}",
                component.semantic.kind,
                component.semantic.index
            );
        }
    }
    Ok(())
}

fn semantic_is_required_in_target(semantic: Semantic) -> bool {
    matches!(semantic.kind, 0 | 6 | 7)
}

fn append_stream_data(
    gpu: &mut Vec<u8>,
    target: &mut StreamRecord,
    target_version: u32,
    request: StreamAppendRequest<'_>,
) -> crate::Result<AppendedSection> {
    let old_vertices = meaningful_vertex_data(gpu, target)?.to_vec();
    let old_indices = decode_indices(
        gpu,
        target.index_offset,
        target.index_count,
        target.index_type,
    )?;
    let source_indices = decode_index_range(request.source_gpu, request.source, request.section)?;
    if source_indices
        .iter()
        .any(|index| *index >= request.section.vertex_count)
    {
        eyre::bail!("helmet section index exceeds its vertex range");
    }
    let appended_vertices = transcode_vertices(VertexTranscodeRequest {
        source_gpu: request.source_gpu,
        source: request.source,
        source_version: request.source_version,
        section: request.section,
        target,
        target_version,
        transform: request.transform,
    })?;
    let vertex_offset = target.vertex_count;
    let index_offset = target.index_count;
    let mut vertex_buffer = old_vertices;
    vertex_buffer.extend_from_slice(&appended_vertices);
    let mut indices = old_indices;
    indices.extend(source_indices);
    let index_type =
        if target.index_type == 1 || indices.iter().any(|index| *index > u16::MAX as u32) {
            1
        } else {
            0
        };
    let index_buffer = encode_indices(&indices, index_type);

    let vertex_at = append_aligned(gpu, &vertex_buffer);
    let index_at = append_aligned(gpu, &index_buffer);
    target.vertex_count = target
        .vertex_count
        .checked_add(request.section.vertex_count)
        .ok_or_else(|| eyre::eyre!("vertex count overflow"))?;
    target.index_count = target
        .index_count
        .checked_add(request.section.index_count)
        .ok_or_else(|| eyre::eyre!("index count overflow"))?;
    target.vertex_offset = u32::try_from(vertex_at)?;
    target.vertex_size = u32::try_from(vertex_buffer.len())?;
    target.index_offset = u32::try_from(index_at)?;
    target.index_size = u32::try_from(index_buffer.len())?;
    target.index_type = index_type;
    Ok(AppendedSection {
        vertex_offset,
        vertex_count: request.section.vertex_count,
        index_offset,
        index_count: request.section.index_count,
    })
}

/// Return only declared vertices, excluding alignment padding at the buffer tail.
fn meaningful_vertex_data<'a>(gpu: &'a [u8], stream: &StreamRecord) -> crate::Result<&'a [u8]> {
    let size = stream
        .vertex_count
        .checked_mul(stream.vertex_stride)
        .ok_or_else(|| eyre::eyre!("body vertex buffer size overflow"))?;
    if size > stream.vertex_size {
        eyre::bail!("body vertex count exceeds its buffer size");
    }
    checked_slice(gpu, stream.vertex_offset, size, "body vertex buffer")
}

struct VertexTranscodeRequest<'a> {
    source_gpu: &'a [u8],
    source: &'a StreamRecord,
    source_version: u32,
    section: MeshSection,
    target: &'a StreamRecord,
    target_version: u32,
    transform: &'a [f32; 16],
}

fn transcode_vertices(request: VertexTranscodeRequest<'_>) -> crate::Result<Vec<u8>> {
    let VertexTranscodeRequest {
        source_gpu,
        source,
        source_version,
        section,
        target,
        target_version,
        transform,
    } = request;
    let capacity = section.vertex_count as usize * target.vertex_stride as usize;
    let mut output = Vec::with_capacity(capacity);
    for local_index in 0..section.vertex_count {
        let source_index = section.vertex_offset + local_index;
        let source_start =
            source.vertex_offset as usize + source_index as usize * source.vertex_stride as usize;
        let before = output.len();
        for target_component in &target.components {
            let mut value = if let Some(source_component) = source
                .components
                .iter()
                .find(|component| component.semantic == target_component.semantic)
            {
                let size = component_size(source_version, source_component.format).unwrap_or(0);
                let bytes = checked_slice_usize(
                    source_gpu,
                    source_start + source_component.offset,
                    size,
                    "helmet vertex component",
                )?;
                decode(source_version, source_component.format, bytes)?
            } else {
                default_value(target_component.semantic)
            };
            value = match target_component.semantic.kind {
                0 => transform_position(value, transform),
                1 => transform_direction(value, transform, true),
                2 | 3 => transform_direction(value, transform, false),
                _ => value,
            };
            encode(target_version, target_component.format, value, &mut output)?;
        }
        output.resize(before + target.vertex_stride as usize, 0);
    }
    Ok(output)
}

fn section_used_bone_slots(
    source: &ParsedUnit,
    stream: &StreamRecord,
    section: SourceSection,
) -> crate::Result<HashSet<usize>> {
    let indices = stream
        .components
        .iter()
        .find(|item| item.semantic.kind == 6)
        .ok_or_else(|| eyre::eyre!("helmet stream has no bone indices"))?;
    let weights = stream
        .components
        .iter()
        .find(|item| item.semantic.kind == 7);
    let mesh_section = source.meshes[section.mesh_index].sections[section.section_index];
    let mut used = HashSet::new();
    for local in 0..mesh_section.vertex_count {
        let vertex = mesh_section.vertex_offset + local;
        let bone_indices = decode_vertex_component(
            VertexComponentRead {
                source,
                stream,
                component: indices,
                vertex,
            },
            "helmet bone indices",
        )?;
        let bone_weights = weights
            .map(|component| {
                decode_vertex_component(
                    VertexComponentRead {
                        source,
                        stream,
                        component,
                        vertex,
                    },
                    "helmet bone weights",
                )
            })
            .transpose()?
            .map(float_values);
        used.extend(active_bone_indices(uint_values(bone_indices), bone_weights));
    }
    Ok(used)
}

struct VertexComponentRead<'a> {
    source: &'a ParsedUnit,
    stream: &'a StreamRecord,
    component: &'a Component,
    vertex: u32,
}

fn decode_vertex_component(
    request: VertexComponentRead<'_>,
    label: &str,
) -> crate::Result<super::vertex::Value> {
    let start = request.stream.vertex_offset as usize
        + request.vertex as usize * request.stream.vertex_stride as usize
        + request.component.offset;
    let size = component_size(request.source.version, request.component.format).unwrap_or(0);
    decode(
        request.source.version,
        request.component.format,
        checked_slice_usize(&request.source.gpu, start, size, label)?,
    )
}

fn active_bone_indices(indices: [u32; 4], weights: Option<[f32; 4]>) -> Vec<usize> {
    indices
        .into_iter()
        .enumerate()
        .filter(|(channel, _)| weights.is_none_or(|values| values[*channel] > 0.000_001))
        .map(|(_, index)| index as usize)
        .collect()
}

fn validate_stream_range(stream: &StreamRecord, gpu: &[u8]) -> crate::Result<()> {
    let vertex_bytes = (stream.vertex_count as usize)
        .checked_mul(stream.vertex_stride as usize)
        .ok_or_else(|| eyre::eyre!("vertex buffer size overflow"))?;
    if vertex_bytes > stream.vertex_size as usize {
        eyre::bail!("vertex count exceeds StreamInfo vertex buffer size");
    }
    checked_slice(
        gpu,
        stream.vertex_offset,
        stream.vertex_size,
        "vertex buffer",
    )?;
    let index_bytes = stream.index_count as usize * index_stride(stream.index_type)?;
    if index_bytes > stream.index_size as usize {
        eyre::bail!("index count exceeds StreamInfo index buffer size");
    }
    checked_slice(gpu, stream.index_offset, stream.index_size, "index buffer")?;
    Ok(())
}

fn validate_mesh_sections(unit: &ParsedUnit, mesh: &MeshRecord) -> crate::Result<()> {
    let stream = &unit.streams[mesh.stream_index];
    for section in &mesh.sections {
        if section.material_index as usize >= mesh.material_slots.len() {
            eyre::bail!("mesh section material index is out of bounds");
        }
        if section
            .vertex_offset
            .checked_add(section.vertex_count)
            .is_none_or(|end| end > stream.vertex_count)
        {
            eyre::bail!("mesh section vertex range is out of bounds");
        }
        if section
            .index_offset
            .checked_add(section.index_count)
            .is_none_or(|end| end > stream.index_count)
        {
            eyre::bail!("mesh section index range is out of bounds");
        }
    }
    Ok(())
}

fn validate_bone_record(bone: &BoneRecord, transform_count: usize) -> crate::Result<()> {
    if bone
        .real_indices
        .iter()
        .any(|index| *index >= transform_count)
    {
        eyre::bail!("BoneInfo references a missing transform");
    }
    if bone
        .remaps
        .iter()
        .flatten()
        .any(|index| *index as usize >= bone.real_indices.len())
    {
        eyre::bail!("BoneInfo remap references a missing real bone");
    }
    Ok(())
}

fn parse_transforms(toc: &[u8]) -> crate::Result<Vec<TransformRecord>> {
    let offset = required_offset(toc, TRANSFORM_OFFSET_FIELD, "TransformInfo")?;
    let count = read_u32(toc, offset, "transform count")? as usize;
    let matrices = offset + 16 + count * 64;
    let hashes = matrices + count * 64 + count * 4;
    require_range(toc, offset, 16 + count * 136, "TransformInfo")?;
    (0..count)
        .map(|index| {
            let mut matrix = [0.0f32; 16];
            for component in 0..16 {
                matrix[component] =
                    f32::from_bits(LE::read_u32(&toc[matrices + index * 64 + component * 4..]));
            }
            Ok(TransformRecord {
                matrix,
                name_hash: LE::read_u32(&toc[hashes + index * 4..]),
            })
        })
        .collect()
}

fn parse_bones(toc: &[u8], start: usize, end: usize) -> crate::Result<Vec<BoneRecord>> {
    let count = read_u32(toc, start, "BoneInfo count")? as usize;
    require_range(toc, start + 4, count * 4, "BoneInfo offsets")?;
    let offsets = (0..count)
        .map(|index| LE::read_u32(&toc[start + 4 + index * 4..]) as usize)
        .collect::<Vec<_>>();
    validate_offsets(&offsets, start, end)?;
    offsets
        .iter()
        .enumerate()
        .map(|(index, relative)| {
            let record_start = start + relative;
            let record_end = offsets
                .get(index + 1)
                .map(|next| start + next)
                .unwrap_or(end);
            parse_bone_record(&toc[record_start..record_end])
        })
        .collect()
}

fn parse_bone_record(bytes: &[u8]) -> crate::Result<BoneRecord> {
    require_range(bytes, 0, 16, "BoneInfo header")?;
    let count = LE::read_u32(bytes) as usize;
    let matrix_offset = LE::read_u32(&bytes[4..]) as usize;
    let real_offset = LE::read_u32(&bytes[8..]) as usize;
    let remap_offset = LE::read_u32(&bytes[12..]) as usize;
    require_range(bytes, matrix_offset, count * 64, "BoneInfo matrices")?;
    require_range(bytes, real_offset, count * 4, "BoneInfo real indices")?;
    let real_indices = (0..count)
        .map(|index| LE::read_u32(&bytes[real_offset + index * 4..]) as usize)
        .collect();
    let remap_count = read_u32(bytes, remap_offset, "BoneInfo remap count")? as usize;
    require_range(
        bytes,
        remap_offset + 4,
        remap_count * 8,
        "BoneInfo remap headers",
    )?;
    let mut remaps = Vec::with_capacity(remap_count);
    for index in 0..remap_count {
        let relative = LE::read_u32(&bytes[remap_offset + 4 + index * 8..]) as usize;
        let length = LE::read_u32(&bytes[remap_offset + 8 + index * 8..]) as usize;
        let at = remap_offset + relative;
        require_range(bytes, at, length * 4, "BoneInfo remap")?;
        remaps.push(
            (0..length)
                .map(|entry| LE::read_u32(&bytes[at + entry * 4..]))
                .collect(),
        );
    }
    Ok(BoneRecord {
        raw: bytes.to_vec(),
        dirty: false,
        real_indices,
        remaps,
    })
}

fn build_bone_record(record: &BoneRecord) -> crate::Result<Vec<u8>> {
    if !record.dirty {
        return Ok(record.raw.clone());
    }
    let mut output = record.raw.clone();
    output.resize(align_up(output.len(), 4), 0);
    let remap_offset = output.len();
    output.resize(remap_offset + 4 + record.remaps.len() * 8, 0);
    LE::write_u32(&mut output[12..], u32::try_from(remap_offset)?);
    LE::write_u32(
        &mut output[remap_offset..],
        u32::try_from(record.remaps.len())?,
    );
    for (index, remap) in record.remaps.iter().enumerate() {
        let relative = output.len() - remap_offset;
        LE::write_u32(
            &mut output[remap_offset + 4 + index * 8..],
            u32::try_from(relative)?,
        );
        LE::write_u32(
            &mut output[remap_offset + 8 + index * 8..],
            u32::try_from(remap.len())?,
        );
        output.extend(remap.iter().flat_map(|value| value.to_le_bytes()));
    }
    Ok(output)
}

fn build_bone_table(absolute_offset: usize, records: &[BoneRecord]) -> crate::Result<Vec<u8>> {
    let mut output = vec![0u8; 4 + records.len() * 4];
    LE::write_u32(&mut output, u32::try_from(records.len())?);
    for (index, record) in records.iter().enumerate() {
        let at = output.len();
        LE::write_u32(&mut output[4 + index * 4..], u32::try_from(at)?);
        output.extend_from_slice(&build_bone_record(record)?);
    }
    output.resize(
        align_up(absolute_offset + output.len(), 16) - absolute_offset,
        0,
    );
    Ok(output)
}

fn parse_materials(toc: &[u8], offset: usize) -> crate::Result<Vec<(u32, u64)>> {
    let count = read_u32(toc, offset, "material count")? as usize;
    require_range(toc, offset + 4, count * 12, "materials")?;
    let ids = offset + 4 + count * 4;
    Ok((0..count)
        .map(|index| {
            (
                LE::read_u32(&toc[offset + 4 + index * 4..]),
                LE::read_u64(&toc[ids + index * 8..]),
            )
        })
        .collect())
}

fn material_table_end(toc: &[u8], offset: usize) -> crate::Result<usize> {
    let count = read_u32(toc, offset, "material count")? as usize;
    let end = offset + 4 + count * 12;
    require_range(toc, offset, end - offset, "materials")?;
    Ok(end)
}

fn build_material_table(materials: &[(u32, u64)]) -> crate::Result<Vec<u8>> {
    let mut output = Vec::with_capacity(4 + materials.len() * 12);
    output.extend_from_slice(&u32::try_from(materials.len())?.to_le_bytes());
    output.extend(materials.iter().flat_map(|(slot, _)| slot.to_le_bytes()));
    output.extend(materials.iter().flat_map(|(_, id)| id.to_le_bytes()));
    Ok(output)
}

fn read_record_table(
    toc: &[u8],
    start: usize,
    end: usize,
    minimum: usize,
    has_trailer: bool,
) -> crate::Result<(Vec<RawRecord>, u32)> {
    let count = read_u32(toc, start, "record count")? as usize;
    require_range(
        toc,
        start + 4,
        count * 8 + usize::from(has_trailer) * 4,
        "record table",
    )?;
    let offsets = (0..count)
        .map(|index| LE::read_u32(&toc[start + 4 + index * 4..]) as usize)
        .collect::<Vec<_>>();
    let markers = (0..count)
        .map(|index| LE::read_u32(&toc[start + 4 + count * 4 + index * 4..]))
        .collect::<Vec<_>>();
    validate_offsets(&offsets, start, end)?;
    let records = offsets
        .iter()
        .enumerate()
        .map(|(index, relative)| {
            let record_start = start + relative;
            let record_end = offsets
                .get(index + 1)
                .map(|next| start + next)
                .unwrap_or(end);
            if record_end < record_start + minimum {
                eyre::bail!("record {index} is shorter than {minimum} bytes");
            }
            Ok(RawRecord {
                marker: markers[index],
                bytes: toc[record_start..record_end].to_vec(),
            })
        })
        .collect::<crate::Result<Vec<_>>>()?;
    let trailer = if has_trailer {
        LE::read_u32(&toc[start + 4 + count * 8..])
    } else {
        0
    };
    Ok((records, trailer))
}

fn build_record_table(
    records: &[RawRecord],
    trailer: Option<u32>,
    absolute_offset: usize,
) -> crate::Result<Vec<u8>> {
    let header = 4 + records.len() * 8 + usize::from(trailer.is_some()) * 4;
    let record_start = align_up(absolute_offset + header, 16) - absolute_offset;
    let mut output = vec![0u8; record_start];
    LE::write_u32(&mut output, u32::try_from(records.len())?);
    for (index, record) in records.iter().enumerate() {
        let relative = u32::try_from(output.len())?;
        LE::write_u32(&mut output[4 + index * 4..], relative);
        LE::write_u32(
            &mut output[4 + records.len() * 4 + index * 4..],
            record.marker,
        );
        output.extend_from_slice(&record.bytes);
    }
    if let Some(value) = trailer {
        LE::write_u32(&mut output[4 + records.len() * 8..], value);
    }
    Ok(output)
}

fn replace_range_and_offsets(
    output: &mut Vec<u8>,
    start: usize,
    end: usize,
    replacement: &[u8],
) -> crate::Result<()> {
    require_range(
        output,
        start,
        end.saturating_sub(start),
        "Unit replacement range",
    )?;
    let delta = replacement.len() as i64 - (end - start) as i64;
    output.splice(start..end, replacement.iter().copied());
    if delta == 0 {
        return Ok(());
    }
    for field in (0x30..=0x70).step_by(4) {
        require_range(output, field, 4, "Unit top-level offset")?;
        let value = LE::read_u32(&output[field..]) as usize;
        if value > start {
            let adjusted = i64::try_from(value)? + delta;
            LE::write_u32(&mut output[field..], u32::try_from(adjusted)?);
        }
    }
    Ok(())
}

fn parse_section(bytes: &[u8], offset: usize) -> crate::Result<MeshSection> {
    require_range(bytes, offset, SECTION_SIZE, "mesh section")?;
    Ok(MeshSection {
        material_index: LE::read_u32(&bytes[offset..]),
        vertex_offset: LE::read_u32(&bytes[offset + 4..]),
        vertex_count: LE::read_u32(&bytes[offset + 8..]),
        index_offset: LE::read_u32(&bytes[offset + 12..]),
        index_count: LE::read_u32(&bytes[offset + 16..]),
        group_index: LE::read_u32(&bytes[offset + 20..]),
    })
}

fn write_section(output: &mut Vec<u8>, section: MeshSection) {
    output.extend_from_slice(&section.material_index.to_le_bytes());
    output.extend_from_slice(&section.vertex_offset.to_le_bytes());
    output.extend_from_slice(&section.vertex_count.to_le_bytes());
    output.extend_from_slice(&section.index_offset.to_le_bytes());
    output.extend_from_slice(&section.index_count.to_le_bytes());
    output.extend_from_slice(&section.group_index.to_le_bytes());
}

fn decode_index_range(
    gpu: &[u8],
    stream: &StreamRecord,
    section: MeshSection,
) -> crate::Result<Vec<u32>> {
    let stride = index_stride(stream.index_type)?;
    let start = stream.index_offset as usize + section.index_offset as usize * stride;
    let bytes = checked_slice_usize(
        gpu,
        start,
        section.index_count as usize * stride,
        "helmet section indices",
    )?;
    decode_index_bytes(bytes, stream.index_type)
}

fn decode_indices(gpu: &[u8], offset: u32, count: u32, index_type: u32) -> crate::Result<Vec<u32>> {
    let stride = index_stride(index_type)?;
    let bytes = checked_slice_usize(
        gpu,
        offset as usize,
        count as usize * stride,
        "body index buffer",
    )?;
    decode_index_bytes(bytes, index_type)
}

fn decode_index_bytes(bytes: &[u8], index_type: u32) -> crate::Result<Vec<u32>> {
    match index_type {
        0 => Ok(bytes
            .chunks_exact(2)
            .map(|chunk| LE::read_u16(chunk) as u32)
            .collect()),
        1 => Ok(bytes.chunks_exact(4).map(LE::read_u32).collect()),
        _ => eyre::bail!("unsupported index type {index_type}"),
    }
}

fn encode_indices(indices: &[u32], index_type: u32) -> Vec<u8> {
    if index_type == 1 {
        return indices
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
    }
    indices
        .iter()
        .flat_map(|value| (*value as u16).to_le_bytes())
        .collect()
}

fn index_stride(index_type: u32) -> crate::Result<usize> {
    match index_type {
        0 => Ok(2),
        1 => Ok(4),
        _ => eyre::bail!("unsupported index type {index_type}"),
    }
}

fn append_aligned(output: &mut Vec<u8>, bytes: &[u8]) -> usize {
    output.resize(align_up(output.len(), 16), 0);
    let offset = output.len();
    output.extend_from_slice(bytes);
    offset
}

fn unique_slot(materials: &[(u32, u64)], original: u32, material: u64) -> u32 {
    for attempt in 0u32.. {
        let text =
            format!("hd2-mod-tools/head-merge/slot/{original:08x}/{material:016x}/{attempt}");
        let candidate = murmur32(text.as_bytes(), 0);
        if !materials.iter().any(|(slot, _)| *slot == candidate) {
            return candidate;
        }
    }
    unreachable!()
}

#[allow(dead_code)]
fn deterministic_file_id(namespace: &str) -> u64 {
    murmur64(namespace.as_bytes(), 0)
}

fn required_offset(bytes: &[u8], field: usize, label: &str) -> crate::Result<usize> {
    let value = read_u32(bytes, field, label)? as usize;
    if value == 0 || value >= bytes.len() {
        eyre::bail!("{label} offset is missing or out of bounds");
    }
    Ok(value)
}

fn validate_offsets(offsets: &[usize], table: usize, end: usize) -> crate::Result<()> {
    let mut previous = 0usize;
    for offset in offsets {
        if *offset < previous || table + *offset >= end {
            eyre::bail!("record offsets are invalid");
        }
        previous = *offset;
    }
    Ok(())
}

fn checked_slice<'a>(
    bytes: &'a [u8],
    offset: u32,
    len: u32,
    label: &str,
) -> crate::Result<&'a [u8]> {
    checked_slice_usize(bytes, offset as usize, len as usize, label)
}

fn checked_slice_usize<'a>(
    bytes: &'a [u8],
    offset: usize,
    len: usize,
    label: &str,
) -> crate::Result<&'a [u8]> {
    require_range(bytes, offset, len, label)?;
    Ok(&bytes[offset..offset + len])
}

fn read_u64(bytes: &[u8], offset: usize, label: &str) -> crate::Result<u64> {
    require_range(bytes, offset, 8, label)?;
    Ok(LE::read_u64(&bytes[offset..]))
}

fn read_u32(bytes: &[u8], offset: usize, label: &str) -> crate::Result<u32> {
    require_range(bytes, offset, 4, label)?;
    Ok(LE::read_u32(&bytes[offset..]))
}

fn read_i32(bytes: &[u8], offset: usize, label: &str) -> crate::Result<i32> {
    require_range(bytes, offset, 4, label)?;
    Ok(LE::read_i32(&bytes[offset..]))
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) -> crate::Result<()> {
    require_range(bytes, offset, 4, "u32 field")?;
    LE::write_u32(&mut bytes[offset..], value);
    Ok(())
}

fn require_range(bytes: &[u8], start: usize, len: usize, label: &str) -> crate::Result<()> {
    let end = start
        .checked_add(len)
        .ok_or_else(|| eyre::eyre!("{label} range overflow"))?;
    if end > bytes.len() {
        eyre::bail!("{label} is out of bounds: {start}..{end} > {}", bytes.len());
    }
    Ok(())
}

fn align_up(value: usize, alignment: usize) -> usize {
    value.div_ceil(alignment) * alignment
}

#[cfg(test)]
mod tests;
