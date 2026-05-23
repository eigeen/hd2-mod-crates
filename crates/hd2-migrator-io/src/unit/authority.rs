//! Authoritative armor Unit-part mapping.
//!
//! The bundled table maps armor names to their manually verified major Unit
//! parts. These assignments are applied before geometry matching.

use crate::archive::StreamToc;
use crate::constants::UNIT_ID;
use eyre::WrapErr;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

const BUILTIN_ARMOR_MAPPING_JSON: &str = hd2_migrator_data::ARMOR_MAPPING_JSON;

pub const EXPECTED_PART_LABELS: [&str; 10] = [
    "slim waist",
    "stocky right arm",
    "slim right arm",
    "stocky body",
    "stocky waist",
    "slim left arm",
    "stocky left arm",
    "slim body",
    "left leg",
    "right leg",
];

#[derive(Debug, Clone, Deserialize)]
pub struct ArmorMappingTable {
    #[serde(flatten)]
    armors: HashMap<String, ArmorPartMap>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArmorPartMap {
    #[serde(flatten)]
    parts: HashMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitAuthorityMatch {
    pub source_file_id: u64,
    pub target_file_id: u64,
    pub part_label: String,
}

impl ArmorMappingTable {
    pub fn bundled() -> crate::Result<Self> {
        BUILTIN_ARMOR_MAPPING_JSON.parse()
    }

    pub fn load(path: &Path) -> crate::Result<Self> {
        let bytes = std::fs::read(path)
            .wrap_err_with(|| format!("read armor mapping {}", path.display()))?;
        let text = String::from_utf8(bytes)
            .wrap_err_with(|| format!("decode armor mapping as UTF-8 {}", path.display()))?;
        text.parse()
    }

    pub fn armor_count(&self) -> usize {
        self.armors.len()
    }

    pub fn armor(&self, name: &str) -> Option<&ArmorPartMap> {
        self.armors.get(name)
    }

    fn validate(&self) -> crate::Result<()> {
        for (name, parts) in &self.armors {
            parts.validate(name)?;
        }
        Ok(())
    }
}

impl std::str::FromStr for ArmorMappingTable {
    type Err = eyre::Report;

    fn from_str(text: &str) -> std::result::Result<Self, Self::Err> {
        let table: Self = serde_json::from_str(text)?;
        table.validate()?;
        Ok(table)
    }
}

impl ArmorPartMap {
    pub fn get(&self, part_label: &str) -> Option<u64> {
        self.parts.get(part_label).copied()
    }

    pub fn all_file_ids(&self) -> Vec<u64> {
        self.parts.values().copied().collect()
    }

    fn validate(&self, armor_name: &str) -> crate::Result<()> {
        for label in EXPECTED_PART_LABELS {
            if !self.parts.contains_key(label) {
                eyre::bail!("armor mapping {armor_name:?} is missing part {label:?}");
            }
        }
        for label in self.parts.keys() {
            if !EXPECTED_PART_LABELS.contains(&label.as_str()) {
                eyre::bail!("armor mapping {armor_name:?} has unknown part {label:?}");
            }
        }
        let unique_ids: HashSet<u64> = self.parts.values().copied().collect();
        if unique_ids.len() != self.parts.len() {
            eyre::bail!("armor mapping {armor_name:?} has duplicate Unit FileIDs");
        }
        Ok(())
    }

    fn part_for_source(&self, source_file_id: u64) -> Option<&str> {
        self.parts
            .iter()
            .find_map(|(part, file_id)| (*file_id == source_file_id).then_some(part.as_str()))
    }
}

pub fn build_authority_matches(
    patch: &StreamToc,
    target: &StreamToc,
    table: &ArmorMappingTable,
    source_name: &str,
    target_name: &str,
) -> Vec<UnitAuthorityMatch> {
    let Some(source_parts) = table.armor(source_name) else {
        return Vec::new();
    };
    let Some(target_parts) = table.armor(target_name) else {
        return Vec::new();
    };
    let target_unit_ids = unit_file_ids(target);
    patch
        .entries
        .iter()
        .filter(|entry| entry.type_id == UNIT_ID)
        .filter_map(|entry| {
            let part_label = source_parts.part_for_source(entry.file_id)?;
            let target_file_id = target_parts.get(part_label)?;
            if !target_unit_ids.contains(&target_file_id) {
                return None;
            }
            Some(UnitAuthorityMatch {
                source_file_id: entry.file_id,
                target_file_id,
                part_label: part_label.to_string(),
            })
        })
        .collect()
}

fn unit_file_ids(toc: &StreamToc) -> HashSet<u64> {
    toc.entries
        .iter()
        .filter(|entry| entry.type_id == UNIT_ID)
        .map(|entry| entry.file_id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::TocEntry;

    const RAW_TABLE: &str = r#"{
        "Source Armor": {
            "slim waist": 1,
            "stocky right arm": 2,
            "slim right arm": 3,
            "stocky body": 4,
            "stocky waist": 5,
            "slim left arm": 6,
            "stocky left arm": 7,
            "slim body": 8,
            "left leg": 9,
            "right leg": 10
        },
        "Target Armor": {
            "slim waist": 101,
            "stocky right arm": 102,
            "slim right arm": 103,
            "stocky body": 104,
            "stocky waist": 105,
            "slim left arm": 106,
            "stocky left arm": 107,
            "slim body": 108,
            "left leg": 109,
            "right leg": 110
        }
    }"#;

    #[test]
    fn bundled_table_parses() {
        let table = ArmorMappingTable::bundled().unwrap();
        assert!(table.armor_count() > 100);
    }

    #[test]
    fn builds_patch_unit_matches_by_part_label() {
        let table = RAW_TABLE.parse::<ArmorMappingTable>().unwrap();
        let mut patch = StreamToc::default();
        patch.entries.push(TocEntry::new(8, UNIT_ID));
        patch.entries.push(TocEntry::new(9, UNIT_ID));

        let mut target = StreamToc::default();
        target.entries.push(TocEntry::new(108, UNIT_ID));
        target.entries.push(TocEntry::new(109, UNIT_ID));

        let matches =
            build_authority_matches(&patch, &target, &table, "Source Armor", "Target Armor");

        assert_eq!(
            matches,
            vec![
                UnitAuthorityMatch {
                    source_file_id: 8,
                    target_file_id: 108,
                    part_label: "slim body".to_string(),
                },
                UnitAuthorityMatch {
                    source_file_id: 9,
                    target_file_id: 109,
                    part_label: "left leg".to_string(),
                },
            ]
        );
    }

    #[test]
    fn skips_authority_target_missing_from_archive() {
        let table = RAW_TABLE.parse::<ArmorMappingTable>().unwrap();
        let mut patch = StreamToc::default();
        patch.entries.push(TocEntry::new(8, UNIT_ID));
        let target = StreamToc::default();

        let matches =
            build_authority_matches(&patch, &target, &table, "Source Armor", "Target Armor");

        assert!(matches.is_empty());
    }
}
