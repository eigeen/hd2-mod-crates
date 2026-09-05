//! Platform-neutral equipment part graph analysis for imported patches.

use super::equipment::{
    EquipmentCategory, WebEquipmentInspection, WebEquipmentOption, inspect_equipment,
    inspect_equipment_with_source, list_equipment_options, patch_unit_ids,
};
use super::migration::PatchBytes;
use crate::io::DataSource;
use crate::unit::authority::ArmorMappingTable;
use crate::unit::helmet_authority::HelmetMappingTable;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub const EQUIPMENT_GRAPH_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebEquipmentPatchAnalysis {
    pub inspection: WebEquipmentInspection,
    pub equipment_graph: WebEquipmentPartGraph,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebEquipmentPartGraph {
    pub schema_version: u16,
    pub patch: WebEquipmentGraphSummary,
    pub equipments: Vec<WebGraphEquipment>,
    pub components: Vec<WebGraphComponent>,
    pub relations: Vec<WebEquipmentPartRelation>,
    pub diagnostics: Vec<WebEquipmentGraphDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebEquipmentGraphSummary {
    pub name: String,
    pub unit_count: usize,
    pub mapped_unit_count: usize,
    pub unmapped_unit_count: usize,
    pub equipment_count: usize,
    pub relation_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebGraphEquipment {
    pub id: String,
    pub category: EquipmentCategory,
    pub hash: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebGraphComponent {
    pub id: String,
    pub file_id: String,
    pub kind: WebGraphComponentKind,
    pub present_in_patch: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WebGraphComponentKind {
    Unit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebEquipmentPartRelation {
    pub id: String,
    pub equipment_id: String,
    pub component_id: String,
    pub role: EquipmentPartRole,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum EquipmentPartRole {
    SlimWaist,
    StockyRightArm,
    SlimRightArm,
    StockyBody,
    StockyWaist,
    SlimLeftArm,
    StockyLeftArm,
    SlimBody,
    LeftLeg,
    RightLeg,
    Helmet,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebEquipmentGraphDiagnostic {
    pub code: WebEquipmentGraphDiagnosticCode,
    pub component_id: String,
    pub file_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WebEquipmentGraphDiagnosticCode {
    UnmappedUnit,
}

#[derive(Debug, Clone)]
struct CatalogRelation {
    category: EquipmentCategory,
    equipment_hash: Option<String>,
    equipment_name: String,
    role: EquipmentPartRole,
}

#[derive(Debug, Default)]
struct EquipmentGraphCatalog {
    by_unit: BTreeMap<u64, Vec<CatalogRelation>>,
}

impl EquipmentGraphCatalog {
    fn bundled() -> crate::Result<Self> {
        let hashes = equipment_hashes(list_equipment_options()?);
        let mut catalog = Self::default();
        catalog.add_armor_mappings(&hashes, ArmorMappingTable::bundled()?)?;
        catalog.add_helmet_mappings(&hashes, HelmetMappingTable::bundled()?);
        catalog.sort_relations();
        Ok(catalog)
    }

    fn add_armor_mappings(
        &mut self,
        hashes: &HashMap<(EquipmentCategory, String), String>,
        table: ArmorMappingTable,
    ) -> crate::Result<()> {
        for (name, parts) in table.entries() {
            for (label, unit_id) in parts.parts() {
                self.add_relation(
                    *unit_id,
                    catalog_relation(EquipmentCategory::Armor, name, label, hashes)?,
                );
            }
        }
        Ok(())
    }

    fn add_helmet_mappings(
        &mut self,
        hashes: &HashMap<(EquipmentCategory, String), String>,
        table: HelmetMappingTable,
    ) {
        for (name, unit_id) in table.entries() {
            let relation = CatalogRelation {
                category: EquipmentCategory::Helmet,
                equipment_hash: equipment_hash(EquipmentCategory::Helmet, name, hashes),
                equipment_name: name.to_owned(),
                role: EquipmentPartRole::Helmet,
            };
            self.add_relation(unit_id, relation);
        }
    }

    fn add_relation(&mut self, unit_id: u64, relation: CatalogRelation) {
        self.by_unit.entry(unit_id).or_default().push(relation);
    }

    fn sort_relations(&mut self) {
        for relations in self.by_unit.values_mut() {
            relations.sort_by(|left, right| {
                category_order(left.category)
                    .cmp(&category_order(right.category))
                    .then_with(|| left.equipment_name.cmp(&right.equipment_name))
                    .then_with(|| left.role.cmp(&right.role))
            });
        }
    }
}

pub fn analyze_equipment_patch(patch: &PatchBytes) -> crate::Result<WebEquipmentPatchAnalysis> {
    Ok(WebEquipmentPatchAnalysis {
        inspection: inspect_equipment(patch)?,
        equipment_graph: build_equipment_part_graph(patch)?,
    })
}

pub async fn analyze_equipment_patch_with_source<S: DataSource + ?Sized>(
    patch: &PatchBytes,
    source: &S,
) -> crate::Result<WebEquipmentPatchAnalysis> {
    Ok(WebEquipmentPatchAnalysis {
        inspection: inspect_equipment_with_source(patch, source).await?,
        equipment_graph: build_equipment_part_graph(patch)?,
    })
}

pub fn build_equipment_part_graph(patch: &PatchBytes) -> crate::Result<WebEquipmentPartGraph> {
    let unit_ids = patch_unit_ids(&patch.toc)?;
    let catalog = EquipmentGraphCatalog::bundled()?;
    Ok(build_graph(&patch.name, &unit_ids, &catalog))
}

fn build_graph(
    patch_name: &str,
    unit_ids: &HashSet<u64>,
    catalog: &EquipmentGraphCatalog,
) -> WebEquipmentPartGraph {
    let mut builder = EquipmentGraphBuilder::new(patch_name, unit_ids.len());
    let sorted_ids = unit_ids.iter().copied().collect::<BTreeSet<_>>();
    for unit_id in sorted_ids {
        builder.add_unit(unit_id, catalog.by_unit.get(&unit_id));
    }
    builder.finish()
}

struct EquipmentGraphBuilder {
    patch_name: String,
    unit_count: usize,
    mapped_units: usize,
    equipments: BTreeMap<String, WebGraphEquipment>,
    components: BTreeMap<String, WebGraphComponent>,
    relations: BTreeMap<String, WebEquipmentPartRelation>,
    diagnostics: Vec<WebEquipmentGraphDiagnostic>,
}

impl EquipmentGraphBuilder {
    fn new(patch_name: &str, unit_count: usize) -> Self {
        Self {
            patch_name: patch_name.to_owned(),
            unit_count,
            mapped_units: 0,
            equipments: BTreeMap::new(),
            components: BTreeMap::new(),
            relations: BTreeMap::new(),
            diagnostics: Vec::new(),
        }
    }

    fn add_unit(&mut self, unit_id: u64, catalog_relations: Option<&Vec<CatalogRelation>>) {
        let component = graph_component(unit_id);
        self.components
            .insert(component.id.clone(), component.clone());
        let Some(catalog_relations) = catalog_relations else {
            self.diagnostics.push(unmapped_diagnostic(&component));
            return;
        };
        self.mapped_units += 1;
        for catalog_relation in catalog_relations {
            self.add_catalog_relation(&component, catalog_relation);
        }
    }

    fn add_catalog_relation(&mut self, component: &WebGraphComponent, catalog: &CatalogRelation) {
        let equipment = graph_equipment(catalog);
        let relation = graph_relation(&equipment, component, catalog.role);
        self.equipments.insert(equipment.id.clone(), equipment);
        self.relations.insert(relation.id.clone(), relation);
    }

    fn finish(self) -> WebEquipmentPartGraph {
        let equipment_count = self.equipments.len();
        let relation_count = self.relations.len();
        WebEquipmentPartGraph {
            schema_version: EQUIPMENT_GRAPH_SCHEMA_VERSION,
            patch: WebEquipmentGraphSummary {
                name: self.patch_name,
                unit_count: self.unit_count,
                mapped_unit_count: self.mapped_units,
                unmapped_unit_count: self.unit_count - self.mapped_units,
                equipment_count,
                relation_count,
            },
            equipments: self.equipments.into_values().collect(),
            components: self.components.into_values().collect(),
            relations: self.relations.into_values().collect(),
            diagnostics: self.diagnostics,
        }
    }
}

fn equipment_hashes(
    options: Vec<WebEquipmentOption>,
) -> HashMap<(EquipmentCategory, String), String> {
    options
        .into_iter()
        .map(|option| ((option.category, option.name), option.hash))
        .collect()
}

fn equipment_hash(
    category: EquipmentCategory,
    name: &str,
    hashes: &HashMap<(EquipmentCategory, String), String>,
) -> Option<String> {
    hashes.get(&(category, name.to_owned())).cloned()
}

fn catalog_relation(
    category: EquipmentCategory,
    name: &str,
    label: &str,
    hashes: &HashMap<(EquipmentCategory, String), String>,
) -> crate::Result<CatalogRelation> {
    Ok(CatalogRelation {
        category,
        equipment_hash: equipment_hash(category, name, hashes),
        equipment_name: name.to_owned(),
        role: EquipmentPartRole::from_mapping_label(label)?,
    })
}

impl EquipmentPartRole {
    pub(super) fn from_mapping_label(label: &str) -> crate::Result<Self> {
        Ok(match label {
            "slim waist" => Self::SlimWaist,
            "stocky right arm" => Self::StockyRightArm,
            "slim right arm" => Self::SlimRightArm,
            "stocky body" => Self::StockyBody,
            "stocky waist" => Self::StockyWaist,
            "slim left arm" => Self::SlimLeftArm,
            "stocky left arm" => Self::StockyLeftArm,
            "slim body" => Self::SlimBody,
            "left leg" => Self::LeftLeg,
            "right leg" => Self::RightLeg,
            unknown => eyre::bail!("unsupported equipment part label {unknown:?}"),
        })
    }

    pub(super) fn key(self) -> &'static str {
        match self {
            Self::SlimWaist => "slimWaist",
            Self::StockyRightArm => "stockyRightArm",
            Self::SlimRightArm => "slimRightArm",
            Self::StockyBody => "stockyBody",
            Self::StockyWaist => "stockyWaist",
            Self::SlimLeftArm => "slimLeftArm",
            Self::StockyLeftArm => "stockyLeftArm",
            Self::SlimBody => "slimBody",
            Self::LeftLeg => "leftLeg",
            Self::RightLeg => "rightLeg",
            Self::Helmet => "helmet",
        }
    }
}

fn graph_equipment(catalog: &CatalogRelation) -> WebGraphEquipment {
    let id = equipment_id(catalog);
    WebGraphEquipment {
        id,
        category: catalog.category,
        hash: catalog.equipment_hash.clone(),
        name: catalog.equipment_name.clone(),
    }
}

fn equipment_id(catalog: &CatalogRelation) -> String {
    let category = category_key(catalog.category);
    match &catalog.equipment_hash {
        Some(hash) => format!("equipment:{category}:{hash}"),
        None => format!("equipment:{category}:name:{}", catalog.equipment_name),
    }
}

fn graph_component(unit_id: u64) -> WebGraphComponent {
    WebGraphComponent {
        id: component_id(unit_id),
        file_id: file_id_hex(unit_id),
        kind: WebGraphComponentKind::Unit,
        present_in_patch: true,
    }
}

fn graph_relation(
    equipment: &WebGraphEquipment,
    component: &WebGraphComponent,
    role: EquipmentPartRole,
) -> WebEquipmentPartRelation {
    WebEquipmentPartRelation {
        id: format!("relation:{}:{}:{}", equipment.id, role.key(), component.id),
        equipment_id: equipment.id.clone(),
        component_id: component.id.clone(),
        role,
    }
}

fn unmapped_diagnostic(component: &WebGraphComponent) -> WebEquipmentGraphDiagnostic {
    WebEquipmentGraphDiagnostic {
        code: WebEquipmentGraphDiagnosticCode::UnmappedUnit,
        component_id: component.id.clone(),
        file_id: component.file_id.clone(),
    }
}

fn component_id(unit_id: u64) -> String {
    format!("unit:{unit_id:016x}")
}

fn file_id_hex(unit_id: u64) -> String {
    format!("0x{unit_id:016x}")
}

fn category_key(category: EquipmentCategory) -> &'static str {
    match category {
        EquipmentCategory::Armor => "armor",
        EquipmentCategory::Helmet => "helmet",
    }
}

fn category_order(category: EquipmentCategory) -> u8 {
    match category {
        EquipmentCategory::Armor => 0,
        EquipmentCategory::Helmet => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_unit_produces_one_component_with_multiple_consumers() {
        let unit_id = u64::MAX - 7;
        let catalog = test_catalog(unit_id, &["Armor A", "Armor B"]);
        let graph = build_graph("example.patch_0", &HashSet::from([unit_id]), &catalog);

        assert_eq!(graph.components.len(), 1);
        assert_eq!(graph.equipments.len(), 2);
        assert_eq!(graph.relations.len(), 2);
        assert!(
            graph
                .relations
                .iter()
                .all(|relation| { relation.component_id == "unit:fffffffffffffff8" })
        );
    }

    #[test]
    fn serializes_large_file_ids_as_exact_hex_strings() {
        let unit_id = u64::MAX - 7;
        let graph = build_graph(
            "example.patch_0",
            &HashSet::from([unit_id]),
            &test_catalog(unit_id, &["Armor A"]),
        );
        let value = serde_json::to_value(graph).expect("serialize graph");

        assert_eq!(
            value["components"][0]["fileId"],
            serde_json::Value::String("0xfffffffffffffff8".to_owned())
        );
    }

    #[test]
    fn unmapped_units_remain_visible_with_diagnostics() {
        let graph = build_graph(
            "example.patch_0",
            &HashSet::from([42]),
            &EquipmentGraphCatalog::default(),
        );

        assert_eq!(graph.patch.unmapped_unit_count, 1);
        assert_eq!(graph.components[0].file_id, "0x000000000000002a");
        assert_eq!(
            graph.diagnostics[0].code,
            WebEquipmentGraphDiagnosticCode::UnmappedUnit
        );
    }

    #[test]
    fn bundled_catalog_contains_cross_equipment_sharing() {
        let catalog = EquipmentGraphCatalog::bundled().expect("bundled catalog");
        let shared_count = catalog
            .by_unit
            .values()
            .filter(|relations| relations.len() > 1)
            .count();

        assert!(shared_count > 100);
    }

    #[test]
    fn analyzes_real_patch_fixture_with_shared_equipment_parts() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test_files/PH56&PH-9/9ba626afa44a3aa3.patch_0");
        let patch = PatchBytes {
            name: "9ba626afa44a3aa3.patch_0".to_owned(),
            toc: std::fs::read(path).expect("read real patch fixture"),
            gpu: Vec::new(),
            stream: Vec::new(),
        };

        let graph = build_equipment_part_graph(&patch).expect("analyze real patch fixture");

        assert_eq!(graph.patch.unit_count, 26);
        assert_eq!(graph.components.len(), 26);
        assert_eq!(graph.patch.mapped_unit_count, 12);
        assert_eq!(graph.patch.relation_count, 20);
        assert_eq!(graph.patch.equipment_count, 2);
        assert_eq!(shared_component_count(&graph.relations), 8);
    }

    fn shared_component_count(relations: &[WebEquipmentPartRelation]) -> usize {
        let mut counts = HashMap::<&str, usize>::new();
        for relation in relations {
            *counts.entry(&relation.component_id).or_default() += 1;
        }
        counts.values().filter(|count| **count > 1).count()
    }

    fn test_catalog(unit_id: u64, names: &[&str]) -> EquipmentGraphCatalog {
        let relations = names
            .iter()
            .map(|name| CatalogRelation {
                category: EquipmentCategory::Armor,
                equipment_hash: Some(format!("hash-{name}")),
                equipment_name: (*name).to_owned(),
                role: EquipmentPartRole::SlimBody,
            })
            .collect();
        EquipmentGraphCatalog {
            by_unit: BTreeMap::from([(unit_id, relations)]),
        }
    }
}
