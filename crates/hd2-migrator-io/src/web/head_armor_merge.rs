//! Compose a selected helmet's visible meshes into mapped armor body Units.

use super::{EquipmentCategory, PatchBytes, list_equipment_options};
use crate::archive::{StreamToc, TocEntry};
use crate::constants::{MATERIAL_ID, TEX_ID, UNIT_ID};
use crate::hashing::murmur64;
use crate::refs::{list_material_refs, rewrite_material};
use crate::unit::authority::ArmorMappingTable;
use crate::unit::helmet_authority::HelmetMappingTable;
use crate::unit::mesh::{merge_visible_head_into_body, visible_material_file_ids};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeadArmorMergeOptions {
    pub helmet_hash: String,
    pub armor_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeadArmorMergeSkippedPart {
    pub body_variant: String,
    pub lod: i32,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeadArmorMergedPart {
    pub body_variant: String,
    pub lod: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeadArmorMergeSummary {
    pub helmet_hash: String,
    pub helmet_name: String,
    pub armor_hash: String,
    pub armor_name: String,
    pub merged_body_units: usize,
    pub merged_lods: usize,
    pub merged_parts: Vec<HeadArmorMergedPart>,
    pub skipped_parts: Vec<HeadArmorMergeSkippedPart>,
    pub copied_resources: usize,
    pub remapped_conflicts: usize,
    pub removed_helmet_units: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct HeadArmorMergeResult {
    pub patch: PatchBytes,
    pub summary: HeadArmorMergeSummary,
}

/// Merge one selected helmet into the Slim and Stocky body Units of an armor Patch.
pub fn merge_head_into_armor(
    armor_patch: PatchBytes,
    head_patch: Option<PatchBytes>,
    options: HeadArmorMergeOptions,
) -> crate::Result<HeadArmorMergeResult> {
    let selection = Selection::resolve(&options)?;
    let mut armor = parse_patch(&armor_patch)?;
    let head = match &head_patch {
        Some(patch) => parse_patch(patch)?,
        None => armor.clone(),
    };
    let helmet = find_entry(&head, selection.helmet_unit_id, UNIT_ID, "helmet Unit")?.clone();
    let mut summary = selection.summary(&options);
    let dependency_request = DependencyCopyRequest {
        source: &head,
        helmet: &helmet,
        separate_patch: head_patch.is_some(),
    };
    let material_map = copy_head_dependencies(dependency_request, &mut armor, &mut summary)?;
    merge_body_variants(&mut armor, &helmet, &selection, &material_map, &mut summary);
    if summary.merged_lods == 0 {
        let reasons = summary
            .skipped_parts
            .iter()
            .map(|part| format!("{} LOD {}: {}", part.body_variant, part.lod, part.reason))
            .collect::<Vec<_>>()
            .join("; ");
        eyre::bail!("No armor LOD could be merged: {reasons}");
    }
    summary.removed_helmet_units = remove_selected_helmet(&mut armor, selection.helmet_unit_id);
    let patch = serialize_and_validate(armor, armor_patch.name)?;
    Ok(HeadArmorMergeResult { patch, summary })
}

struct Selection {
    helmet_name: String,
    armor_name: String,
    helmet_unit_id: u64,
    body_units: [(String, u64); 2],
}

impl Selection {
    fn resolve(options: &HeadArmorMergeOptions) -> crate::Result<Self> {
        let equipment = list_equipment_options()?;
        let helmet_name =
            selected_name(&equipment, EquipmentCategory::Helmet, &options.helmet_hash)?;
        let armor_name = selected_name(&equipment, EquipmentCategory::Armor, &options.armor_hash)?;
        let helmet_unit_id = HelmetMappingTable::bundled()?
            .unit_id(&helmet_name)
            .ok_or_else(|| eyre::eyre!("Helmet {helmet_name:?} has no Unit mapping"))?;
        let armor_table = ArmorMappingTable::bundled()?;
        let armor = armor_table
            .armor(&armor_name)
            .ok_or_else(|| eyre::eyre!("Armor {armor_name:?} has no Unit mapping"))?;
        Ok(Self {
            helmet_name,
            armor_name,
            helmet_unit_id,
            body_units: [
                ("slim body".to_owned(), required_body(armor, "slim body")?),
                (
                    "stocky body".to_owned(),
                    required_body(armor, "stocky body")?,
                ),
            ],
        })
    }

    fn summary(&self, options: &HeadArmorMergeOptions) -> HeadArmorMergeSummary {
        HeadArmorMergeSummary {
            helmet_hash: options.helmet_hash.clone(),
            helmet_name: self.helmet_name.clone(),
            armor_hash: options.armor_hash.clone(),
            armor_name: self.armor_name.clone(),
            merged_body_units: 0,
            merged_lods: 0,
            merged_parts: Vec::new(),
            skipped_parts: Vec::new(),
            copied_resources: 0,
            remapped_conflicts: 0,
            removed_helmet_units: 0,
            warnings: Vec::new(),
        }
    }
}

fn selected_name(
    equipment: &[super::WebEquipmentOption],
    category: EquipmentCategory,
    hash: &str,
) -> crate::Result<String> {
    equipment
        .iter()
        .find(|option| option.category == category && option.hash == hash)
        .map(|option| option.name.clone())
        .ok_or_else(|| eyre::eyre!("Selected equipment hash {hash} was not found"))
}

fn required_body(parts: &crate::unit::authority::ArmorPartMap, label: &str) -> crate::Result<u64> {
    parts
        .get(label)
        .ok_or_else(|| eyre::eyre!("Armor mapping is missing {label}"))
}

fn parse_patch(patch: &PatchBytes) -> crate::Result<StreamToc> {
    StreamToc::from_buffers(&patch.toc, &patch.gpu, &patch.stream, patch.name.clone())
}

fn find_entry<'a>(
    archive: &'a StreamToc,
    file_id: u64,
    type_id: u64,
    label: &str,
) -> crate::Result<&'a TocEntry> {
    archive
        .entries
        .iter()
        .find(|entry| entry.file_id == file_id && entry.type_id == type_id)
        .ok_or_else(|| eyre::eyre!("Selected Patch does not contain {label} 0x{file_id:016x}"))
}

fn merge_body_variants(
    armor: &mut StreamToc,
    helmet: &TocEntry,
    selection: &Selection,
    materials: &HashMap<u64, u64>,
    summary: &mut HeadArmorMergeSummary,
) {
    for (variant, unit_id) in &selection.body_units {
        merge_body_variant(
            armor,
            BodyMergeRequest {
                helmet,
                variant,
                unit_id: *unit_id,
                materials,
            },
            summary,
        );
    }
}

struct BodyMergeRequest<'a> {
    helmet: &'a TocEntry,
    variant: &'a str,
    unit_id: u64,
    materials: &'a HashMap<u64, u64>,
}

fn merge_body_variant(
    armor: &mut StreamToc,
    request: BodyMergeRequest<'_>,
    summary: &mut HeadArmorMergeSummary,
) {
    let Some(index) = armor
        .entries
        .iter()
        .position(|entry| entry.file_id == request.unit_id && entry.type_id == UNIT_ID)
    else {
        summary.skipped_parts.push(skipped(
            request.variant,
            -1,
            "Armor Patch does not contain this body variant",
        ));
        return;
    };
    let result = match merge_visible_head_into_body(
        &armor.entries[index],
        request.helmet,
        request.materials,
    ) {
        Ok(result) => result,
        Err(error) => {
            summary
                .skipped_parts
                .push(skipped(request.variant, -1, error.to_string()));
            return;
        }
    };
    let merged_count = result.merged_lods.len();
    if merged_count > 0 {
        summary.merged_body_units += 1;
        summary.merged_lods += merged_count;
        armor.entries[index] = result.entry;
    }
    summary.merged_parts.extend(
        result
            .merged_lods
            .into_iter()
            .map(|lod| HeadArmorMergedPart {
                body_variant: request.variant.to_owned(),
                lod,
            }),
    );
    summary.skipped_parts.extend(
        result
            .skipped_lods
            .into_iter()
            .map(|item| skipped(request.variant, item.lod, item.reason)),
    );
}

fn skipped(variant: &str, lod: i32, reason: impl Into<String>) -> HeadArmorMergeSkippedPart {
    HeadArmorMergeSkippedPart {
        body_variant: variant.to_owned(),
        lod,
        reason: reason.into(),
    }
}

struct DependencyCopyRequest<'a> {
    source: &'a StreamToc,
    helmet: &'a TocEntry,
    separate_patch: bool,
}

fn copy_head_dependencies(
    request: DependencyCopyRequest<'_>,
    output: &mut StreamToc,
    summary: &mut HeadArmorMergeSummary,
) -> crate::Result<HashMap<u64, u64>> {
    let material_ids = visible_material_file_ids(request.helmet)?;
    if !request.separate_patch {
        return Ok(material_ids.into_iter().map(|id| (id, id)).collect());
    }
    let mut copier = DependencyCopier::new(request.source, output, summary);
    copier.copy_materials(&material_ids)
}

struct DependencyCopier<'a> {
    source: &'a StreamToc,
    output: &'a mut StreamToc,
    summary: &'a mut HeadArmorMergeSummary,
}

impl<'a> DependencyCopier<'a> {
    fn new(
        source: &'a StreamToc,
        output: &'a mut StreamToc,
        summary: &'a mut HeadArmorMergeSummary,
    ) -> Self {
        Self {
            source,
            output,
            summary,
        }
    }

    fn copy_materials(&mut self, material_ids: &[u64]) -> crate::Result<HashMap<u64, u64>> {
        let mut materials = HashMap::new();
        for material_id in material_ids {
            let Some(material) = self.source_entry(*material_id, MATERIAL_ID).cloned() else {
                self.summary.warnings.push(format!(
                    "Head material 0x{material_id:016x} is not in the head Patch; keeping its reference"
                ));
                materials.insert(*material_id, *material_id);
                continue;
            };
            let textures = self.copy_textures(&material);
            let mut rewritten = material;
            rewritten.toc_data = rewrite_material(&rewritten.toc_data, &textures);
            let output_id = self.insert_resource(rewritten, *material_id);
            materials.insert(*material_id, output_id);
        }
        Ok(materials)
    }

    fn copy_textures(&mut self, material: &TocEntry) -> HashMap<u64, u64> {
        let mut textures = HashMap::new();
        for texture_id in list_material_refs(&material.toc_data) {
            let Some(texture) = self.source_entry(texture_id, TEX_ID).cloned() else {
                self.summary.warnings.push(format!(
                    "Head texture 0x{texture_id:016x} is not in the head Patch; keeping its reference"
                ));
                continue;
            };
            textures.insert(texture_id, self.insert_resource(texture, texture_id));
        }
        textures
    }

    fn source_entry(&self, file_id: u64, type_id: u64) -> Option<&TocEntry> {
        self.source
            .entries
            .iter()
            .find(|entry| entry.file_id == file_id && entry.type_id == type_id)
    }

    fn insert_resource(&mut self, mut entry: TocEntry, original_id: u64) -> u64 {
        if let Some(existing) = self.output_entry(original_id, entry.type_id) {
            if entries_equal(existing, &entry) {
                return original_id;
            }
            self.summary.remapped_conflicts += 1;
            let (file_id, already_present) = self.conflict_file_id(&entry, original_id);
            if already_present {
                return file_id;
            }
            entry.file_id = file_id;
        }
        let output_id = entry.file_id;
        self.output.entries.push(entry);
        self.summary.copied_resources += 1;
        output_id
    }

    fn output_entry(&self, file_id: u64, type_id: u64) -> Option<&TocEntry> {
        self.output
            .entries
            .iter()
            .find(|entry| entry.file_id == file_id && entry.type_id == type_id)
    }

    fn conflict_file_id(&self, entry: &TocEntry, original_id: u64) -> (u64, bool) {
        for attempt in 0_u32.. {
            let key = format!(
                "hd2-mod-tools/head-merge/{:016x}/{original_id:016x}/{attempt}",
                entry.type_id
            );
            let candidate = murmur64(key.as_bytes(), 0);
            let Some(existing) = self.output_entry(candidate, entry.type_id) else {
                return (candidate, false);
            };
            let mut remapped = entry.clone();
            remapped.file_id = candidate;
            if entries_equal(existing, &remapped) {
                return (candidate, true);
            }
        }
        unreachable!("u64 FileID space exhausted")
    }
}

fn entries_equal(left: &TocEntry, right: &TocEntry) -> bool {
    left.type_id == right.type_id
        && left.file_id == right.file_id
        && left.unknown1 == right.unknown1
        && left.unknown2 == right.unknown2
        && left.unknown3 == right.unknown3
        && left.unknown4 == right.unknown4
        && left.toc_data == right.toc_data
        && left.gpu_data == right.gpu_data
        && left.stream_data == right.stream_data
}

fn remove_selected_helmet(archive: &mut StreamToc, helmet_unit_id: u64) -> usize {
    let before = archive.entries.len();
    archive
        .entries
        .retain(|entry| entry.type_id != UNIT_ID || entry.file_id != helmet_unit_id);
    before - archive.entries.len()
}

fn serialize_and_validate(mut archive: StreamToc, name: String) -> crate::Result<PatchBytes> {
    archive.name = name.clone();
    let (toc, gpu, stream) = archive.serialize();
    StreamToc::from_buffers(&toc, &gpu, &stream, name.clone())?;
    Ok(PatchBytes {
        name,
        toc,
        gpu,
        stream,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_dependency_is_reused() {
        let texture = fixture_entry(7, TEX_ID, b"same");
        let source = archive(vec![texture.clone()]);
        let mut output = archive(vec![texture]);
        let mut summary = fixture_summary();
        let mut copier = DependencyCopier::new(&source, &mut output, &mut summary);

        assert_eq!(copier.insert_resource(source.entries[0].clone(), 7), 7);
        assert_eq!(output.entries.len(), 1);
        assert_eq!(summary.copied_resources, 0);
    }

    #[test]
    fn conflicting_dependency_gets_stable_file_id() {
        let source_entry = fixture_entry(7, TEX_ID, b"head");
        let source = archive(vec![source_entry.clone()]);
        let first = copy_conflict(&source, source_entry.clone());
        let second = copy_conflict(&source, source_entry);

        assert_eq!(first, second);
        assert_ne!(first, 7);
    }

    #[test]
    fn selected_helmet_unit_is_removed_only_by_type_and_id() {
        let mut output = archive(vec![
            fixture_entry(7, UNIT_ID, b"helmet"),
            fixture_entry(7, MATERIAL_ID, b"material"),
            fixture_entry(8, UNIT_ID, b"armor"),
        ]);
        assert_eq!(remove_selected_helmet(&mut output, 7), 1);
        assert_eq!(output.entries.len(), 2);
    }

    fn copy_conflict(source: &StreamToc, entry: TocEntry) -> u64 {
        let mut output = archive(vec![fixture_entry(7, TEX_ID, b"armor")]);
        let mut summary = fixture_summary();
        let mut copier = DependencyCopier::new(source, &mut output, &mut summary);
        let id = copier.insert_resource(entry, 7);
        assert_eq!(summary.remapped_conflicts, 1);
        id
    }

    fn fixture_entry(file_id: u64, type_id: u64, bytes: &[u8]) -> TocEntry {
        let mut entry = TocEntry::new(file_id, type_id);
        entry.toc_data = bytes.to_vec();
        entry
    }

    fn archive(entries: Vec<TocEntry>) -> StreamToc {
        StreamToc {
            entries,
            ..StreamToc::default()
        }
    }

    fn fixture_summary() -> HeadArmorMergeSummary {
        HeadArmorMergeSummary {
            helmet_hash: String::new(),
            helmet_name: String::new(),
            armor_hash: String::new(),
            armor_name: String::new(),
            merged_body_units: 0,
            merged_lods: 0,
            merged_parts: Vec::new(),
            skipped_parts: Vec::new(),
            copied_resources: 0,
            remapped_conflicts: 0,
            removed_helmet_units: 0,
            warnings: Vec::new(),
        }
    }
}
