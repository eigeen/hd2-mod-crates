//! Compose a selected helmet's visible meshes into mapped armor body Units.

use super::{EquipmentCategory, PatchBytes, list_equipment_options};
use crate::archive::{StreamToc, TocEntry};
use crate::constants::{MATERIAL_ID, TEX_ID, UNIT_ID};
use crate::hashing::murmur64;
use crate::io::DataSource;
use crate::io::archive_async::{load_archive, open_bundle};
use crate::refs::{list_material_refs, rewrite_material};
use crate::unit::authority::ArmorMappingTable;
use crate::unit::helmet_authority::HelmetMappingTable;
use crate::unit::mesh::{merge_visible_head_into_body, visible_material_file_ids};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

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
    pub restored_body_units: usize,
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
    merge_head_into_armor_with_base(armor_patch, head_patch, options, None)
}

/// Fill missing body Units from game data before merging the selected helmet.
pub async fn merge_head_into_armor_with_source<S: DataSource + ?Sized>(
    armor_patch: PatchBytes,
    head_patch: Option<PatchBytes>,
    options: HeadArmorMergeOptions,
    source: &S,
) -> crate::Result<HeadArmorMergeResult> {
    if !needs_base_archive(&armor_patch, &options)? {
        return merge_head_into_armor(armor_patch, head_patch, options);
    }
    let bundle = open_bundle(source).await?;
    let base = load_archive(source, bundle.as_ref(), &options.armor_hash).await?;
    merge_head_into_armor_with_base(armor_patch, head_patch, options, Some(base))
}

fn merge_head_into_armor_with_base(
    armor_patch: PatchBytes,
    head_patch: Option<PatchBytes>,
    options: HeadArmorMergeOptions,
    base_armor: Option<StreamToc>,
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
    let sources = BodyMergeSources {
        patch: &armor,
        base: base_armor.as_ref(),
        helmet: &helmet,
    };
    let updates = merge_body_variants(sources, &selection, &material_map, &mut summary);
    ensure_all_body_variants_merged(&summary, selection.body_units.len())?;
    apply_body_updates(&mut armor, updates, &mut summary);
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
            restored_body_units: 0,
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

fn needs_base_archive(
    armor_patch: &PatchBytes,
    options: &HeadArmorMergeOptions,
) -> crate::Result<bool> {
    let selection = Selection::resolve(options)?;
    let armor = parse_patch(armor_patch)?;
    Ok(selection
        .body_units
        .iter()
        .any(|(_, unit_id)| !has_unit(&armor, *unit_id)))
}

fn has_unit(archive: &StreamToc, unit_id: u64) -> bool {
    archive
        .entries
        .iter()
        .any(|entry| entry.file_id == unit_id && entry.type_id == UNIT_ID)
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
    sources: BodyMergeSources<'_>,
    selection: &Selection,
    materials: &HashMap<u64, u64>,
    summary: &mut HeadArmorMergeSummary,
) -> Vec<BodyMergeUpdate> {
    let requests = prepare_body_merge_requests(sources, selection, summary);
    let mut updates = Vec::with_capacity(requests.len());
    for request in requests {
        if let Some(update) = merge_body_variant(&request, materials, summary) {
            updates.push(update);
        }
    }
    updates
}

struct BodyMergeRequest {
    body_index: Option<usize>,
    body: TocEntry,
    helmet: TocEntry,
    variant: String,
}

struct BodyMergeUpdate {
    body_index: Option<usize>,
    entry: TocEntry,
}

struct BodyTarget<'a> {
    variant: &'a str,
    unit_id: u64,
}

#[derive(Clone, Copy)]
struct BodyMergeSources<'a> {
    patch: &'a StreamToc,
    base: Option<&'a StreamToc>,
    helmet: &'a TocEntry,
}

fn prepare_body_merge_requests(
    sources: BodyMergeSources<'_>,
    selection: &Selection,
    summary: &mut HeadArmorMergeSummary,
) -> Vec<BodyMergeRequest> {
    selection
        .body_units
        .iter()
        .filter_map(|(variant, unit_id)| {
            let target = BodyTarget {
                variant,
                unit_id: *unit_id,
            };
            prepare_body_merge_request(sources, target, summary)
        })
        .collect()
}

fn prepare_body_merge_request(
    sources: BodyMergeSources<'_>,
    target: BodyTarget<'_>,
    summary: &mut HeadArmorMergeSummary,
) -> Option<BodyMergeRequest> {
    let body_index = sources
        .patch
        .entries
        .iter()
        .position(|entry| entry.file_id == target.unit_id && entry.type_id == UNIT_ID);
    let body = body_index
        .map(|index| sources.patch.entries[index].clone())
        .or_else(|| find_base_body(sources.base, target.unit_id));
    let Some(body) = body else {
        summary.skipped_parts.push(skipped(
            target.variant,
            -1,
            missing_body_reason(sources.base.is_some()),
        ));
        return None;
    };
    Some(BodyMergeRequest {
        body_index,
        body,
        helmet: copy_unit_data(sources.helmet),
        variant: target.variant.to_owned(),
    })
}

fn find_base_body(base: Option<&StreamToc>, unit_id: u64) -> Option<TocEntry> {
    base?
        .entries
        .iter()
        .find(|entry| entry.file_id == unit_id && entry.type_id == UNIT_ID)
        .cloned()
}

fn missing_body_reason(base_was_loaded: bool) -> &'static str {
    if base_was_loaded {
        return "Selected game archive does not contain this body variant";
    }
    "Armor Mod does not contain this body variant; select the game data directory to fill it"
}

fn merge_body_variant(
    request: &BodyMergeRequest,
    materials: &HashMap<u64, u64>,
    summary: &mut HeadArmorMergeSummary,
) -> Option<BodyMergeUpdate> {
    let result = match merge_visible_head_into_body(&request.body, &request.helmet, materials) {
        Ok(result) => result,
        Err(error) => {
            summary
                .skipped_parts
                .push(skipped(&request.variant, -1, error.to_string()));
            return None;
        }
    };
    if !result.skipped_lods.is_empty() {
        summary.skipped_parts.extend(
            result
                .skipped_lods
                .into_iter()
                .map(|item| skipped(&request.variant, item.lod, item.reason)),
        );
        return None;
    }
    let merged_count = result.merged_lods.len();
    if merged_count > 0 {
        summary.merged_body_units += 1;
        summary.merged_lods += merged_count;
    }
    summary.merged_parts.extend(
        result
            .merged_lods
            .into_iter()
            .map(|lod| HeadArmorMergedPart {
                body_variant: request.variant.clone(),
                lod,
            }),
    );
    Some(BodyMergeUpdate {
        body_index: request.body_index,
        entry: result.entry,
    })
}

fn copy_unit_data(entry: &TocEntry) -> TocEntry {
    let mut copy = entry.clone();
    copy.gpu_data = Arc::from(entry.gpu_data.as_ref());
    copy.stream_data = Arc::from(entry.stream_data.as_ref());
    copy
}

fn ensure_all_body_variants_merged(
    summary: &HeadArmorMergeSummary,
    expected: usize,
) -> crate::Result<()> {
    if summary.merged_body_units == expected && summary.skipped_parts.is_empty() {
        return Ok(());
    }
    let reasons = summary
        .skipped_parts
        .iter()
        .map(|part| format!("{} LOD {}: {}", part.body_variant, part.lod, part.reason))
        .collect::<Vec<_>>()
        .join("; ");
    eyre::bail!("Head was not merged into every body variant and visible LOD: {reasons}")
}

fn apply_body_updates(
    armor: &mut StreamToc,
    updates: Vec<BodyMergeUpdate>,
    summary: &mut HeadArmorMergeSummary,
) {
    for update in updates {
        match update.body_index {
            Some(index) => armor.entries[index] = update.entry,
            None => {
                armor.entries.push(update.entry);
                summary.restored_body_units += 1;
            }
        }
    }
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

    #[test]
    fn body_variants_receive_independent_head_copies() {
        let mut helmet = fixture_entry(7, UNIT_ID, b"helmet");
        helmet.gpu_data = Arc::from([1, 2, 3]);
        let output = archive(vec![
            fixture_entry(8, UNIT_ID, b"slim"),
            fixture_entry(9, UNIT_ID, b"stocky"),
        ]);
        let selection = fixture_selection();
        let mut summary = fixture_summary();
        let sources = BodyMergeSources {
            patch: &output,
            base: None,
            helmet: &helmet,
        };

        let requests = prepare_body_merge_requests(sources, &selection, &mut summary);

        assert_eq!(requests.len(), 2);
        assert!(!Arc::ptr_eq(&requests[0].helmet.gpu_data, &helmet.gpu_data));
        assert!(!Arc::ptr_eq(
            &requests[0].helmet.gpu_data,
            &requests[1].helmet.gpu_data,
        ));
        assert!(summary.skipped_parts.is_empty());
    }

    #[test]
    fn missing_patch_body_is_loaded_from_base_archive() {
        let helmet = fixture_entry(7, UNIT_ID, b"helmet");
        let output = archive(vec![fixture_entry(8, UNIT_ID, b"slim")]);
        let base = archive(vec![fixture_entry(9, UNIT_ID, b"stocky base")]);
        let selection = fixture_selection();
        let mut summary = fixture_summary();
        let sources = BodyMergeSources {
            patch: &output,
            base: Some(&base),
            helmet: &helmet,
        };

        let requests = prepare_body_merge_requests(sources, &selection, &mut summary);

        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].body_index, Some(0));
        assert_eq!(requests[1].body_index, None);
        assert_eq!(requests[1].body.toc_data, b"stocky base");
        assert!(summary.skipped_parts.is_empty());
    }

    #[test]
    fn partial_body_merge_is_rejected() {
        let mut summary = fixture_summary();
        summary.merged_body_units = 1;
        summary.skipped_parts.push(skipped(
            "stocky body",
            -1,
            "Armor Patch does not contain this body variant",
        ));

        let error = ensure_all_body_variants_merged(&summary, 2).unwrap_err();

        assert!(error.to_string().contains("stocky body"));
    }

    #[test]
    fn skipped_visible_lod_is_rejected_after_both_bodies_merge() {
        let mut summary = fixture_summary();
        summary.merged_body_units = 2;
        summary.skipped_parts.push(skipped(
            "slim body",
            2,
            "helmet has no compatible visible mesh",
        ));

        let error = ensure_all_body_variants_merged(&summary, 2).unwrap_err();

        assert!(error.to_string().contains("slim body LOD 2"));
    }

    #[test]
    fn inserted_base_body_is_reported() {
        let mut output = archive(vec![fixture_entry(8, UNIT_ID, b"slim")]);
        let mut summary = fixture_summary();
        let update = BodyMergeUpdate {
            body_index: None,
            entry: fixture_entry(9, UNIT_ID, b"stocky"),
        };

        apply_body_updates(&mut output, vec![update], &mut summary);

        assert_eq!(output.entries.len(), 2);
        assert_eq!(summary.restored_body_units, 1);
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
            restored_body_units: 0,
            merged_lods: 0,
            merged_parts: Vec::new(),
            skipped_parts: Vec::new(),
            copied_resources: 0,
            remapped_conflicts: 0,
            removed_helmet_units: 0,
            warnings: Vec::new(),
        }
    }

    fn fixture_selection() -> Selection {
        Selection {
            helmet_name: String::new(),
            armor_name: String::new(),
            helmet_unit_id: 7,
            body_units: [("slim body".to_owned(), 8), ("stocky body".to_owned(), 9)],
        }
    }
}
