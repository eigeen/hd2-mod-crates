//! Wasm-safe unit metadata helpers used by migration planning.

use hd2_migrator_data::BONEHASH_TEXT;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitCustomizationName {
    pub body_type: String,
    pub slot: String,
    pub weight: String,
    pub piece_type: String,
}

impl UnitCustomizationName {
    pub fn label(&self) -> String {
        format!(
            "{} {} {} {}",
            self.weight, self.body_type, self.slot, self.piece_type
        )
    }
}

pub fn extract_customization_name(toc_data: &[u8]) -> Option<UnitCustomizationName> {
    let strings = ascii_strings(toc_data);
    Some(UnitCustomizationName {
        body_type: last_suffix(&strings, "HelldiverCustomizationBodyType_")?,
        slot: last_suffix(&strings, "HelldiverCustomizationSlot_")?,
        weight: last_suffix(&strings, "HelldiverCustomizationWeight_")?,
        piece_type: last_suffix(&strings, "HelldiverCustomizationPieceType_")?,
    })
}

pub fn bundled_armor_mapping() -> hd2_archive_format::Result<ArmorMappingTable> {
    parse_armor_mapping(hd2_migrator_data::ARMOR_MAPPING_JSON)
}

pub fn parse_armor_mapping(text: &str) -> hd2_archive_format::Result<ArmorMappingTable> {
    let table: ArmorMappingTable =
        serde_json::from_str(text).map_err(|error| hd2_archive_format::error::message(error.to_string()))?;
    table.validate()?;
    Ok(table)
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArmorMappingTable {
    #[serde(flatten)]
    pub armors: HashMap<String, ArmorMappingEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArmorMappingEntry {
    #[serde(flatten)]
    pub parts: HashMap<String, u64>,
}

impl ArmorMappingTable {
    pub fn entry(&self, armor_name: &str) -> Option<&ArmorMappingEntry> {
        self.armors.get(armor_name)
    }

    fn validate(&self) -> hd2_archive_format::Result<()> {
        for (name, entry) in &self.armors {
            entry.validate(name)?;
        }
        Ok(())
    }
}

impl ArmorMappingEntry {
    fn validate(&self, armor_name: &str) -> hd2_archive_format::Result<()> {
        let unique_ids: HashSet<u64> = self.parts.values().copied().collect();
        if unique_ids.len() != self.parts.len() {
            return Err(hd2_archive_format::error::message(format!(
                "armor mapping {armor_name:?} has duplicate Unit FileIDs"
            )));
        }
        Ok(())
    }
}

pub fn bonehash_line_count() -> usize {
    BONEHASH_TEXT.lines().filter(|line| !line.trim().is_empty()).count()
}

fn ascii_strings(data: &[u8]) -> Vec<String> {
    let mut strings = Vec::new();
    let mut current = Vec::new();
    for byte in data {
        if byte.is_ascii_graphic() || *byte == b'_' {
            current.push(*byte);
            continue;
        }
        push_ascii_string(&mut strings, &mut current);
    }
    push_ascii_string(&mut strings, &mut current);
    strings
}

fn push_ascii_string(strings: &mut Vec<String>, current: &mut Vec<u8>) {
    if current.len() >= 4 {
        strings.push(String::from_utf8_lossy(current).into_owned());
    }
    current.clear();
}

fn last_suffix(strings: &[String], prefix: &str) -> Option<String> {
    strings
        .iter()
        .filter_map(|value| value.strip_prefix(prefix))
        .last()
        .map(ToOwned::to_owned)
}
