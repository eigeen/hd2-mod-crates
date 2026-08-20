use super::equipment::{
    EquipmentCategory, WebEquipmentOption, WebMigrationMapping, list_equipment_options, load_toc,
    patch_unit_ids,
};
use super::equipment_graph::{EquipmentPartRole, WebGraphEquipment};
use super::migration::PatchBytes;
use crate::archive::dsar;
use crate::archive::toc_only::TocOnlyPackage;
use crate::constants::{DSAR_MAGIC, UNIT_ID};
use crate::io::{BundleSlicer, DataSource};
use crate::unit::authority::ArmorMappingTable;
use crate::unit::culling::inspect_unit_culling;
use crate::unit::helmet_authority::HelmetMappingTable;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub const MAPPING_PREVIEW_SCHEMA_VERSION: u16 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebEquipmentMappingPreview {
    pub schema_version: u16,
    pub source_equipment: WebGraphEquipment,
    pub target_equipment: WebGraphEquipment,
    pub units: Vec<WebMappingPreviewUnit>,
    pub mappings: Vec<WebUnitMappingPreview>,
    pub summary: WebMappingPreviewSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebMappingPreviewUnit {
    pub id: String,
    pub file_id: String,
    pub present_in_patch: bool,
    pub source_roles: Vec<EquipmentPartRole>,
    pub target_roles: Vec<EquipmentPartRole>,
    pub patch_culling_mesh_count: Option<usize>,
    pub target_culling_mesh_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebUnitMappingPreview {
    pub id: String,
    pub source_unit_id: String,
    pub target_unit_id: String,
    pub role: EquipmentPartRole,
    pub action: WebUnitMappingAction,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WebUnitMappingAction {
    Replace,
    Reuse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebMappingPreviewSummary {
    pub mapped_unit_count: usize,
    pub replaced_unit_count: usize,
    pub unchanged_unit_count: usize,
    pub reused_source_unit_count: usize,
}

#[derive(Debug, Default)]
struct PreviewUnitBuilder {
    present_in_patch: bool,
    source_roles: BTreeSet<EquipmentPartRole>,
    target_roles: BTreeSet<EquipmentPartRole>,
    patch_culling_mesh_count: Option<usize>,
    target_culling_mesh_count: Option<usize>,
}

struct MappingPreviewTables {
    armor: ArmorMappingTable,
    helmet: HelmetMappingTable,
}

impl MappingPreviewTables {
    fn bundled() -> crate::Result<Self> {
        Ok(Self {
            armor: ArmorMappingTable::bundled()?,
            helmet: HelmetMappingTable::bundled()?,
        })
    }
}

/// Build the authoritative Unit mapping for one selected source/target pair.
pub fn preview_equipment_mapping(
    patch: &PatchBytes,
    mapping: &WebMigrationMapping,
) -> crate::Result<WebEquipmentMappingPreview> {
    preview_equipment_mappings(patch, std::slice::from_ref(mapping))?
        .pop()
        .ok_or_else(|| eyre::eyre!("mapping preview unexpectedly returned no result"))
}

/// Build all selected mappings while decoding the Patch and mapping tables once.
pub fn preview_equipment_mappings(
    patch: &PatchBytes,
    mappings: &[WebMigrationMapping],
) -> crate::Result<Vec<WebEquipmentMappingPreview>> {
    let options = list_equipment_options()?;
    let patch_units = patch_unit_ids(&patch.toc)?;
    let culling_counts = patch_culling_counts(&patch.toc)?;
    let tables = MappingPreviewTables::bundled()?;
    mappings
        .iter()
        .map(|mapping| {
            preview_mapping(
                mapping,
                &options,
                &patch_units,
                &culling_counts,
                None,
                &tables,
            )
        })
        .collect()
}

pub async fn preview_equipment_mapping_with_source<S: DataSource + ?Sized>(
    patch: &PatchBytes,
    mapping: &WebMigrationMapping,
    source: &S,
) -> crate::Result<WebEquipmentMappingPreview> {
    preview_equipment_mappings_with_source(patch, std::slice::from_ref(mapping), source)
        .await?
        .pop()
        .ok_or_else(|| eyre::eyre!("mapping preview unexpectedly returned no result"))
}

pub async fn preview_equipment_mappings_with_source<S: DataSource + ?Sized>(
    patch: &PatchBytes,
    mappings: &[WebMigrationMapping],
    source: &S,
) -> crate::Result<Vec<WebEquipmentMappingPreview>> {
    let options = list_equipment_options()?;
    let patch_units = patch_unit_ids(&patch.toc)?;
    let patch_culling = patch_culling_counts(&patch.toc)?;
    let target_culling = load_target_culling_counts(mappings, source).await?;
    let tables = MappingPreviewTables::bundled()?;
    mappings
        .iter()
        .map(|mapping| {
            preview_mapping(
                mapping,
                &options,
                &patch_units,
                &patch_culling,
                target_culling.get(&mapping.target_hash),
                &tables,
            )
        })
        .collect()
}

async fn load_target_culling_counts<S: DataSource + ?Sized>(
    mappings: &[WebMigrationMapping],
    source: &S,
) -> crate::Result<HashMap<String, HashMap<u64, usize>>> {
    let bundle = if source.exists("bundles.nxa").await? {
        Some(BundleSlicer::open(source).await?)
    } else {
        None
    };
    let hashes = mappings
        .iter()
        .map(|mapping| mapping.target_hash.clone())
        .collect::<BTreeSet<_>>();
    let mut counts = HashMap::new();
    for hash in hashes {
        let toc = load_toc(source, bundle.as_ref(), &hash).await?;
        counts.insert(hash, patch_culling_counts(&decompress_toc(toc)?)?);
    }
    Ok(counts)
}

fn decompress_toc(toc: Vec<u8>) -> crate::Result<Vec<u8>> {
    if toc.get(..4) == Some(&DSAR_MAGIC.to_le_bytes()) {
        return dsar::decompress(&toc);
    }
    Ok(toc)
}

fn preview_mapping(
    mapping: &WebMigrationMapping,
    options: &[WebEquipmentOption],
    patch_units: &HashSet<u64>,
    patch_culling: &HashMap<u64, usize>,
    target_culling: Option<&HashMap<u64, usize>>,
    tables: &MappingPreviewTables,
) -> crate::Result<WebEquipmentMappingPreview> {
    let source = find_equipment(options, mapping.category, &mapping.source_hash)?;
    let target = find_equipment(options, mapping.category, &mapping.target_hash)?;
    let parts = authoritative_part_mappings(mapping.category, &source.name, &target.name, tables)?;
    Ok(build_preview(
        source,
        target,
        patch_units,
        patch_culling,
        target_culling.unwrap_or(&HashMap::new()),
        parts,
    ))
}

fn authoritative_part_mappings(
    category: EquipmentCategory,
    source_name: &str,
    target_name: &str,
    tables: &MappingPreviewTables,
) -> crate::Result<Vec<(EquipmentPartRole, u64, u64)>> {
    match category {
        EquipmentCategory::Armor => armor_part_mappings(source_name, target_name, &tables.armor),
        EquipmentCategory::Helmet => helmet_part_mappings(source_name, target_name, &tables.helmet),
    }
}

fn armor_part_mappings(
    source_name: &str,
    target_name: &str,
    table: &ArmorMappingTable,
) -> crate::Result<Vec<(EquipmentPartRole, u64, u64)>> {
    let source = table
        .armor(source_name)
        .ok_or_else(|| eyre::eyre!("armor mapping has no source {source_name:?}"))?;
    let target = table
        .armor(target_name)
        .ok_or_else(|| eyre::eyre!("armor mapping has no target {target_name:?}"))?;
    let mut mappings = source
        .parts()
        .iter()
        .filter_map(|(label, source_id)| {
            let target_id = target.get(label)?;
            Some(
                EquipmentPartRole::from_mapping_label(label)
                    .map(|role| (role, *source_id, target_id)),
            )
        })
        .collect::<crate::Result<Vec<_>>>()?;
    mappings.sort_by_key(|(role, _, _)| *role);
    Ok(mappings)
}

fn helmet_part_mappings(
    source_name: &str,
    target_name: &str,
    table: &HelmetMappingTable,
) -> crate::Result<Vec<(EquipmentPartRole, u64, u64)>> {
    let source_id = table
        .unit_id(source_name)
        .ok_or_else(|| eyre::eyre!("helmet mapping has no source {source_name:?}"))?;
    let target_id = table
        .unit_id(target_name)
        .ok_or_else(|| eyre::eyre!("helmet mapping has no target {target_name:?}"))?;
    Ok(vec![(EquipmentPartRole::Helmet, source_id, target_id)])
}

fn find_equipment<'a>(
    options: &'a [WebEquipmentOption],
    category: EquipmentCategory,
    hash: &str,
) -> crate::Result<&'a WebEquipmentOption> {
    options
        .iter()
        .find(|option| option.category == category && option.hash == hash)
        .ok_or_else(|| eyre::eyre!("equipment archive {hash:?} was not found in {category:?}"))
}

fn build_preview(
    source: &WebEquipmentOption,
    target: &WebEquipmentOption,
    patch_units: &HashSet<u64>,
    patch_culling: &HashMap<u64, usize>,
    target_culling: &HashMap<u64, usize>,
    part_mappings: Vec<(EquipmentPartRole, u64, u64)>,
) -> WebEquipmentMappingPreview {
    let active = part_mappings
        .into_iter()
        .filter(|(_, source_id, _)| patch_units.contains(source_id))
        .collect::<Vec<_>>();
    let source_ids = active
        .iter()
        .map(|(_, source_id, _)| *source_id)
        .collect::<HashSet<_>>();
    let mut units = BTreeMap::<u64, PreviewUnitBuilder>::new();
    let mappings = active
        .iter()
        .map(|(role, source_id, target_id)| {
            record_unit_roles(
                &mut units,
                *source_id,
                *target_id,
                *role,
                patch_units,
                patch_culling,
                target_culling,
            );
            mapping_preview(*source_id, *target_id, *role)
        })
        .collect::<Vec<_>>();
    let summary = preview_summary(&active, &source_ids);
    WebEquipmentMappingPreview {
        schema_version: MAPPING_PREVIEW_SCHEMA_VERSION,
        source_equipment: preview_equipment(source),
        target_equipment: preview_equipment(target),
        units: units.into_iter().map(mapping_preview_unit).collect(),
        mappings,
        summary,
    }
}

fn record_unit_roles(
    units: &mut BTreeMap<u64, PreviewUnitBuilder>,
    source_id: u64,
    target_id: u64,
    role: EquipmentPartRole,
    patch_units: &HashSet<u64>,
    patch_culling: &HashMap<u64, usize>,
    target_culling: &HashMap<u64, usize>,
) {
    let source = units.entry(source_id).or_default();
    source.present_in_patch = true;
    source.source_roles.insert(role);
    source.patch_culling_mesh_count = patch_culling.get(&source_id).copied();
    let target = units.entry(target_id).or_default();
    target.present_in_patch = patch_units.contains(&target_id);
    target.target_roles.insert(role);
    target.target_culling_mesh_count = target_culling.get(&target_id).copied();
}

fn mapping_preview(
    source_id: u64,
    target_id: u64,
    role: EquipmentPartRole,
) -> WebUnitMappingPreview {
    let action = if source_id == target_id {
        WebUnitMappingAction::Reuse
    } else {
        WebUnitMappingAction::Replace
    };
    WebUnitMappingPreview {
        id: format!(
            "mapping:{}:{}:{}",
            role.key(),
            component_id(source_id),
            component_id(target_id)
        ),
        source_unit_id: component_id(source_id),
        target_unit_id: component_id(target_id),
        role,
        action,
    }
}

fn preview_summary(
    mappings: &[(EquipmentPartRole, u64, u64)],
    source_ids: &HashSet<u64>,
) -> WebMappingPreviewSummary {
    WebMappingPreviewSummary {
        mapped_unit_count: mappings.len(),
        replaced_unit_count: mappings
            .iter()
            .filter(|(_, source, target)| source != target)
            .count(),
        unchanged_unit_count: mappings
            .iter()
            .filter(|(_, source, target)| source == target)
            .count(),
        reused_source_unit_count: mappings
            .iter()
            .filter(|(_, source, target)| source != target && source_ids.contains(target))
            .count(),
    }
}

fn mapping_preview_unit((file_id, unit): (u64, PreviewUnitBuilder)) -> WebMappingPreviewUnit {
    WebMappingPreviewUnit {
        id: component_id(file_id),
        file_id: format!("0x{file_id:016x}"),
        present_in_patch: unit.present_in_patch,
        source_roles: unit.source_roles.into_iter().collect(),
        target_roles: unit.target_roles.into_iter().collect(),
        patch_culling_mesh_count: unit.patch_culling_mesh_count,
        target_culling_mesh_count: unit.target_culling_mesh_count,
    }
}

fn patch_culling_counts(toc: &[u8]) -> crate::Result<HashMap<u64, usize>> {
    let package = TocOnlyPackage::parse(toc)?;
    Ok(package
        .entries
        .iter()
        .filter(|entry| entry.type_id == UNIT_ID)
        .filter_map(|entry| {
            inspect_unit_culling(&entry.toc_data)
                .ok()
                .map(|inspection| (entry.file_id, inspection.culling_meshes.len()))
        })
        .collect())
}

fn preview_equipment(option: &WebEquipmentOption) -> WebGraphEquipment {
    WebGraphEquipment {
        id: format!(
            "equipment:{}:{}",
            category_key(option.category),
            option.hash
        ),
        category: option.category,
        hash: Some(option.hash.clone()),
        name: option.name.clone(),
    }
}

fn component_id(file_id: u64) -> String {
    format!("unit:{file_id:016x}")
}

fn category_key(category: EquipmentCategory) -> &'static str {
    match category {
        EquipmentCategory::Armor => "armor",
        EquipmentCategory::Helmet => "helmet",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_matching_another_source_unit_reuses_one_node() {
        let source = option("source", "Source");
        let target = option("target", "Target");
        let preview = build_preview(
            &source,
            &target,
            &HashSet::from([1, 2]),
            &HashMap::new(),
            &HashMap::new(),
            vec![
                (EquipmentPartRole::SlimBody, 1, 2),
                (EquipmentPartRole::StockyBody, 2, 12),
            ],
        );

        assert_eq!(preview.units.len(), 3);
        assert_eq!(preview.summary.reused_source_unit_count, 1);
        let shared = preview
            .units
            .iter()
            .find(|unit| unit.file_id.ends_with("0002"))
            .unwrap();
        assert_eq!(shared.source_roles, vec![EquipmentPartRole::StockyBody]);
        assert_eq!(shared.target_roles, vec![EquipmentPartRole::SlimBody]);
    }

    #[test]
    fn unchanged_mapping_has_one_unit_node() {
        let preview = build_preview(
            &option("source", "Source"),
            &option("target", "Target"),
            &HashSet::from([7]),
            &HashMap::new(),
            &HashMap::new(),
            vec![(EquipmentPartRole::Helmet, 7, 7)],
        );

        assert_eq!(preview.units.len(), 1);
        assert_eq!(preview.mappings[0].action, WebUnitMappingAction::Reuse);
        assert_eq!(preview.summary.unchanged_unit_count, 1);
    }

    #[test]
    fn keeps_patch_and_target_culling_counts_separate() {
        let preview = build_preview(
            &option("source", "Source"),
            &option("target", "Target"),
            &HashSet::from([7]),
            &HashMap::from([(7, 2)]),
            &HashMap::from([(9, 4)]),
            vec![(EquipmentPartRole::Helmet, 7, 9)],
        );

        let source = preview
            .units
            .iter()
            .find(|unit| unit.file_id.ends_with("0007"))
            .unwrap();
        let target = preview
            .units
            .iter()
            .find(|unit| unit.file_id.ends_with("0009"))
            .unwrap();
        assert_eq!(source.patch_culling_mesh_count, Some(2));
        assert_eq!(source.target_culling_mesh_count, None);
        assert_eq!(target.patch_culling_mesh_count, None);
        assert_eq!(target.target_culling_mesh_count, Some(4));
    }

    fn option(hash: &str, name: &str) -> WebEquipmentOption {
        WebEquipmentOption {
            category: EquipmentCategory::Armor,
            hash: hash.to_owned(),
            name: name.to_owned(),
            excluded: false,
        }
    }
}
