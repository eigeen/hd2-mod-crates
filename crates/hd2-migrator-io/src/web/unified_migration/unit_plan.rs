use super::WebMigrationVariant;
use super::unit_behavior::CompiledUnitBehavior;
use crate::unit::authority::ArmorMappingTable;
use crate::unit::direct_mapping::match_unit_parts;
use crate::unit::helmet_authority::HelmetMappingTable;
use crate::web::equipment::{EquipmentCategory, WebMigrationMapping};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UnitMappingEdge {
    pub source_file_id: u64,
    pub target_file_id: u64,
    description: String,
}

impl UnitMappingEdge {
    pub(super) fn described(source_file_id: u64, target_file_id: u64, description: String) -> Self {
        Self {
            source_file_id,
            target_file_id,
            description,
        }
    }

    #[cfg(test)]
    pub(super) fn test_edge(source_file_id: u64, target_file_id: u64) -> Self {
        Self::described(source_file_id, target_file_id, "test edge".to_string())
    }
}

#[derive(Debug, Clone)]
pub(super) struct VariantUnitPlan {
    pub mapping_edges: Vec<Vec<UnitMappingEdge>>,
}

struct UnitPlanTables {
    armor: ArmorMappingTable,
    helmet: HelmetMappingTable,
}

/// Build and validate all authoritative Unit assignments before archive I/O starts.
pub(super) fn build_variant_plans(
    variants: &[WebMigrationVariant],
    behavior: &CompiledUnitBehavior,
) -> crate::Result<Vec<VariantUnitPlan>> {
    let tables = UnitPlanTables {
        armor: ArmorMappingTable::bundled()?,
        helmet: HelmetMappingTable::bundled()?,
    };
    variants
        .iter()
        .map(|variant| build_variant_plan(variant, &tables, behavior))
        .collect()
}

fn build_variant_plan(
    variant: &WebMigrationVariant,
    tables: &UnitPlanTables,
    behavior: &CompiledUnitBehavior,
) -> crate::Result<VariantUnitPlan> {
    let mapping_edges = variant
        .mappings
        .iter()
        .map(|mapping| mapping_edges(mapping, tables))
        .collect::<crate::Result<Vec<_>>>()?;
    validate_target_owners(&variant.mappings, &mapping_edges, behavior)?;
    Ok(VariantUnitPlan { mapping_edges })
}

fn mapping_edges(
    mapping: &WebMigrationMapping,
    tables: &UnitPlanTables,
) -> crate::Result<Vec<UnitMappingEdge>> {
    let source_name = super::archive_name(mapping.category, &mapping.source_hash)?;
    let target_name = super::archive_name(mapping.category, &mapping.target_hash)?;
    match mapping.category {
        EquipmentCategory::Armor => armor_edges(&source_name, &target_name, &tables.armor),
        EquipmentCategory::Helmet => helmet_edges(&source_name, &target_name, &tables.helmet),
    }
}

fn armor_edges(
    source_name: &str,
    target_name: &str,
    table: &ArmorMappingTable,
) -> crate::Result<Vec<UnitMappingEdge>> {
    let (Some(source), Some(target)) = (table.armor(source_name), table.armor(target_name)) else {
        return Ok(Vec::new());
    };
    Ok(match_unit_parts(source.parts(), target.parts())
        .into_iter()
        .map(|edge| {
            UnitMappingEdge::described(
                edge.source_file_id,
                edge.target_file_id,
                format!("Armor {source_name} -> {target_name} ({})", edge.part_label),
            )
        })
        .collect())
}

fn helmet_edges(
    source_name: &str,
    target_name: &str,
    table: &HelmetMappingTable,
) -> crate::Result<Vec<UnitMappingEdge>> {
    let source_file_id = helmet_unit_id(table, source_name)?;
    Ok(vec![UnitMappingEdge::described(
        source_file_id,
        helmet_unit_id(table, target_name)?,
        format!("Helmet {source_name} -> {target_name}"),
    )])
}

fn helmet_unit_id(table: &HelmetMappingTable, name: &str) -> crate::Result<u64> {
    table
        .unit_id(name)
        .ok_or_else(|| eyre::eyre!("helmet mapping not found for {name:?}"))
}

fn validate_target_owners(
    mappings: &[WebMigrationMapping],
    mapping_edges: &[Vec<UnitMappingEdge>],
    behavior: &CompiledUnitBehavior,
) -> crate::Result<()> {
    validate_preferred_sources(mapping_edges, behavior)?;
    let mut owners = HashMap::<u64, (&WebMigrationMapping, UnitMappingEdge)>::new();
    for (mapping, edges) in mappings.iter().zip(mapping_edges) {
        for edge in edges {
            if !behavior.selects_edge(edge) {
                continue;
            }
            validate_target_owner(&mut owners, mapping, edge)?;
        }
    }
    Ok(())
}

fn validate_preferred_sources(
    mapping_edges: &[Vec<UnitMappingEdge>],
    behavior: &CompiledUnitBehavior,
) -> crate::Result<()> {
    for (target_file_id, sources) in candidate_sources_by_target(mapping_edges) {
        let Some(preferred) = behavior.preferred_source(target_file_id) else {
            continue;
        };
        if !sources.contains(&preferred) {
            eyre::bail!(
                "preferred source Unit 0x{preferred:016x} does not map to target FileID 0x{target_file_id:016x}"
            );
        }
    }
    Ok(())
}

fn candidate_sources_by_target(
    mapping_edges: &[Vec<UnitMappingEdge>],
) -> HashMap<u64, std::collections::HashSet<u64>> {
    let mut candidates = HashMap::<u64, std::collections::HashSet<u64>>::new();
    for edge in mapping_edges.iter().flatten() {
        candidates
            .entry(edge.target_file_id)
            .or_default()
            .insert(edge.source_file_id);
    }
    candidates
}

fn validate_target_owner<'a>(
    owners: &mut HashMap<u64, (&'a WebMigrationMapping, UnitMappingEdge)>,
    mapping: &'a WebMigrationMapping,
    edge: &UnitMappingEdge,
) -> crate::Result<()> {
    let Some(previous) = owners.get(&edge.target_file_id) else {
        owners.insert(edge.target_file_id, (mapping, edge.clone()));
        return Ok(());
    };
    if plan_claims_are_compatible(previous, mapping, edge) {
        return Ok(());
    }
    eyre::bail!(
        "combined migration maps different source Units to target FileID 0x{:016x}: {} [0x{:016x}] conflicts with {} [0x{:016x}]",
        edge.target_file_id,
        previous.1.description,
        previous.1.source_file_id,
        edge.description,
        edge.source_file_id,
    )
}

fn plan_claims_are_compatible(
    previous: &(&WebMigrationMapping, UnitMappingEdge),
    mapping: &WebMigrationMapping,
    edge: &UnitMappingEdge,
) -> bool {
    previous.1.source_file_id == edge.source_file_id
        || (previous.0.category == mapping.category
            && previous.0.source_hash == mapping.source_hash
            && previous.0.target_hash != mapping.target_hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::unified_migration::{WebUnitBehaviorOptions, WebUnitConflictResolution};

    const FS_55_ARMOR: &str = "1308bf1fbd277eb2";
    const CE_07_ARMOR: &str = "ffcad1f7ff9888d7";
    const SC_34_ARMOR: &str = "0bb13de574d4acce";
    const B_22_ARMOR: &str = "1120577ed1e095f8";
    const FS_55_STOCKY_RIGHT_ARM: u64 = 12_692_932_953_023_979_462;
    const SHARED_TARGET_RIGHT_ARM: u64 = 4_262_785_967_333_127_230;

    #[test]
    fn allows_repeated_identical_unit_edges() {
        let edges = vec![vec![edge(1, 9, "first"), edge(1, 9, "second")]];
        assert!(
            validate_target_owners(
                &[armor_mapping("a", "b")],
                &edges,
                &CompiledUnitBehavior::default(),
            )
            .is_ok()
        );
    }

    #[test]
    fn rejects_distinct_sources_claiming_one_target() {
        let edges = vec![vec![edge(1, 9, "first"), edge(2, 9, "second")]];
        let error = validate_target_owners(
            &[armor_mapping("a", "b")],
            &edges,
            &CompiledUnitBehavior::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("different source Units"));
        assert!(error.contains("0x0000000000000009"));
    }

    #[test]
    fn preferred_source_resolves_distinct_target_claims() {
        let edges = vec![vec![edge(1, 9, "first"), edge(2, 9, "second")]];
        let behavior = CompiledUnitBehavior::compile(&WebUnitBehaviorOptions {
            conflict_resolutions: vec![WebUnitConflictResolution {
                target_file_id: "9".to_string(),
                preferred_source_file_id: "2".to_string(),
            }],
            ..Default::default()
        })
        .unwrap();

        assert!(validate_target_owners(&[armor_mapping("a", "b")], &edges, &behavior).is_ok());
    }

    #[test]
    fn real_shared_armor_part_is_one_compatible_edge() {
        let variant = variant(&[(FS_55_ARMOR, SC_34_ARMOR), (FS_55_ARMOR, B_22_ARMOR)]);
        let plans = build_variant_plans(&[variant], &CompiledUnitBehavior::default()).unwrap();
        let shared_edges = plans[0]
            .mapping_edges
            .iter()
            .flatten()
            .filter(|edge| edge.target_file_id == SHARED_TARGET_RIGHT_ARM)
            .collect::<Vec<_>>();

        assert_eq!(shared_edges.len(), 2);
        assert!(
            shared_edges
                .iter()
                .all(|edge| edge.source_file_id == FS_55_STOCKY_RIGHT_ARM)
        );
    }

    #[test]
    fn real_distinct_armor_parts_are_rejected_before_migration() {
        let variant = variant(&[(FS_55_ARMOR, B_22_ARMOR), (CE_07_ARMOR, B_22_ARMOR)]);
        let error = build_variant_plans(&[variant], &CompiledUnitBehavior::default())
            .unwrap_err()
            .to_string();

        assert!(error.contains("different source Units"));
    }

    #[test]
    fn fs_55_armor_and_helmet_quick_select_has_no_structural_conflict() {
        let variant = fs_55_quick_select_variant();
        build_variant_plans(&[variant], &CompiledUnitBehavior::default()).unwrap();
    }

    fn fs_55_quick_select_variant() -> WebMigrationVariant {
        let mappings = crate::web::equipment::list_equipment_options()
            .unwrap()
            .into_iter()
            .filter(|option| !option.excluded)
            .filter_map(|option| fs_55_target_mapping(option.category, &option.hash))
            .collect();
        WebMigrationVariant { mappings }
    }

    fn fs_55_target_mapping(
        category: EquipmentCategory,
        target_hash: &str,
    ) -> Option<WebMigrationMapping> {
        let source_hash = match category {
            EquipmentCategory::Armor => FS_55_ARMOR,
            EquipmentCategory::Helmet => "13f9269d08e52cf2",
        };
        (target_hash != source_hash).then(|| WebMigrationMapping {
            category,
            source_hash: source_hash.to_string(),
            target_hash: target_hash.to_string(),
        })
    }

    fn variant(mappings: &[(&str, &str)]) -> WebMigrationVariant {
        WebMigrationVariant {
            mappings: mappings
                .iter()
                .map(|(source_hash, target_hash)| WebMigrationMapping {
                    category: EquipmentCategory::Armor,
                    source_hash: (*source_hash).to_string(),
                    target_hash: (*target_hash).to_string(),
                })
                .collect(),
        }
    }

    fn armor_mapping(source_hash: &str, target_hash: &str) -> WebMigrationMapping {
        WebMigrationMapping {
            category: EquipmentCategory::Armor,
            source_hash: source_hash.to_string(),
            target_hash: target_hash.to_string(),
        }
    }

    fn edge(source_file_id: u64, target_file_id: u64, description: &str) -> UnitMappingEdge {
        UnitMappingEdge::described(source_file_id, target_file_id, description.to_string())
    }
}
