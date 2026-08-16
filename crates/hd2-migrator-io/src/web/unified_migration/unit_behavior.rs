use super::unit_plan::UnitMappingEdge;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebUnitBehaviorOptions {
    #[serde(default)]
    pub disabled_mappings: Vec<WebUnitMappingBehaviorKey>,
    #[serde(default)]
    pub export_overrides: Vec<WebUnitExportOverride>,
    #[serde(default)]
    pub conflict_resolutions: Vec<WebUnitConflictResolution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebUnitMappingBehaviorKey {
    pub source_file_id: String,
    pub target_file_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebUnitExportOverride {
    pub file_id: String,
    pub export: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebUnitConflictResolution {
    pub target_file_id: String,
    pub preferred_source_file_id: String,
}

#[derive(Debug, Clone, Default)]
pub(super) struct CompiledUnitBehavior {
    disabled_mappings: HashSet<(u64, u64)>,
    export_overrides: HashMap<u64, bool>,
    preferred_sources: HashMap<u64, u64>,
}

impl CompiledUnitBehavior {
    pub(super) fn compile(options: &WebUnitBehaviorOptions) -> crate::Result<Self> {
        Ok(Self {
            disabled_mappings: compile_disabled_mappings(&options.disabled_mappings)?,
            export_overrides: compile_export_overrides(&options.export_overrides)?,
            preferred_sources: compile_conflict_resolutions(&options.conflict_resolutions)?,
        })
    }

    pub(super) fn selects_edge(&self, edge: &UnitMappingEdge) -> bool {
        self.export_override(edge.target_file_id) != Some(false)
            && !self
                .disabled_mappings
                .contains(&(edge.source_file_id, edge.target_file_id))
            && self
                .preferred_sources
                .get(&edge.target_file_id)
                .is_none_or(|source| *source == edge.source_file_id)
    }

    pub(super) fn export_override(&self, file_id: u64) -> Option<bool> {
        self.export_overrides.get(&file_id).copied()
    }

    pub(super) fn preferred_source(&self, target_file_id: u64) -> Option<u64> {
        self.preferred_sources.get(&target_file_id).copied()
    }
}

fn compile_disabled_mappings(
    mappings: &[WebUnitMappingBehaviorKey],
) -> crate::Result<HashSet<(u64, u64)>> {
    mappings
        .iter()
        .map(|mapping| {
            Ok((
                parse_file_id(&mapping.source_file_id)?,
                parse_file_id(&mapping.target_file_id)?,
            ))
        })
        .collect()
}

fn compile_export_overrides(
    overrides: &[WebUnitExportOverride],
) -> crate::Result<HashMap<u64, bool>> {
    let mut compiled = HashMap::new();
    for export_override in overrides {
        let file_id = parse_file_id(&export_override.file_id)?;
        insert_unique(
            &mut compiled,
            file_id,
            export_override.export,
            "export override",
        )?;
    }
    Ok(compiled)
}

fn compile_conflict_resolutions(
    resolutions: &[WebUnitConflictResolution],
) -> crate::Result<HashMap<u64, u64>> {
    let mut compiled = HashMap::new();
    for resolution in resolutions {
        let target_file_id = parse_file_id(&resolution.target_file_id)?;
        let source_file_id = parse_file_id(&resolution.preferred_source_file_id)?;
        insert_unique(
            &mut compiled,
            target_file_id,
            source_file_id,
            "conflict resolution",
        )?;
    }
    Ok(compiled)
}

fn insert_unique<T: Copy + PartialEq>(
    values: &mut HashMap<u64, T>,
    file_id: u64,
    value: T,
    label: &str,
) -> crate::Result<()> {
    if values.insert(file_id, value).is_some() {
        eyre::bail!("duplicate Unit {label} for FileID 0x{file_id:016x}");
    }
    Ok(())
}

fn parse_file_id(value: &str) -> crate::Result<u64> {
    let normalized = value.trim().strip_prefix("0x").unwrap_or(value.trim());
    u64::from_str_radix(normalized, 16)
        .map_err(|error| eyre::eyre!("invalid Unit FileID {value:?}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_behavior_preserves_the_legacy_unit_selection() {
        let behavior = CompiledUnitBehavior::compile(&WebUnitBehaviorOptions::default()).unwrap();

        assert!(behavior.selects_edge(&UnitMappingEdge::test_edge(1, 9)));
        assert_eq!(behavior.export_override(9), None);
        assert_eq!(behavior.preferred_source(9), None);
    }

    #[test]
    fn compiles_frontend_hex_file_ids() {
        let behavior = CompiledUnitBehavior::compile(&WebUnitBehaviorOptions {
            disabled_mappings: vec![WebUnitMappingBehaviorKey {
                source_file_id: "0000000000000001".to_string(),
                target_file_id: "0000000000000009".to_string(),
            }],
            export_overrides: vec![WebUnitExportOverride {
                file_id: "0000000000000008".to_string(),
                export: false,
            }],
            conflict_resolutions: vec![WebUnitConflictResolution {
                target_file_id: "0000000000000009".to_string(),
                preferred_source_file_id: "0000000000000002".to_string(),
            }],
        })
        .unwrap();

        assert!(!behavior.selects_edge(&UnitMappingEdge::test_edge(1, 9)));
        assert!(behavior.selects_edge(&UnitMappingEdge::test_edge(2, 9)));
        assert_eq!(behavior.export_override(8), Some(false));
    }

    #[test]
    fn rejects_duplicate_target_rules() {
        let error = CompiledUnitBehavior::compile(&WebUnitBehaviorOptions {
            export_overrides: vec![
                WebUnitExportOverride {
                    file_id: "a".to_string(),
                    export: true,
                },
                WebUnitExportOverride {
                    file_id: "a".to_string(),
                    export: false,
                },
            ],
            ..Default::default()
        })
        .unwrap_err()
        .to_string();

        assert!(error.contains("duplicate Unit export override"));
    }
}
