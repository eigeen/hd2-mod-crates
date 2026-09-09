//! Authoritative helmet-to-Unit mapping.

use eyre::WrapErr;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

const BUILTIN_HELMET_MAPPING_JSON: &str = hd2_migrator_data::HELMET_MAPPING_JSON;
const HELMET_PART_LABEL: &str = "Helmet";

#[derive(Debug, Clone, Deserialize)]
pub struct HelmetMappingTable {
    #[serde(flatten)]
    helmets: HashMap<String, HelmetPartMap>,
}

#[derive(Debug, Clone, Deserialize)]
struct HelmetPartMap {
    #[serde(flatten)]
    parts: HashMap<String, u64>,
}

impl HelmetMappingTable {
    pub fn bundled() -> crate::Result<Self> {
        BUILTIN_HELMET_MAPPING_JSON.parse()
    }

    pub fn load(path: &Path) -> crate::Result<Self> {
        let bytes = std::fs::read(path)
            .wrap_err_with(|| format!("read helmet mapping {}", path.display()))?;
        let text = String::from_utf8(bytes)
            .wrap_err_with(|| format!("decode helmet mapping as UTF-8 {}", path.display()))?;
        text.parse()
    }

    pub fn helmet_count(&self) -> usize {
        self.helmets.len()
    }

    pub fn unit_id(&self, helmet_name: &str) -> Option<u64> {
        self.helmets
            .get(helmet_name)
            .and_then(|mapping| mapping.parts.get(HELMET_PART_LABEL))
            .copied()
    }

    pub(crate) fn entries(&self) -> impl Iterator<Item = (&str, u64)> {
        self.helmets.iter().filter_map(|(name, mapping)| {
            mapping
                .parts
                .get(HELMET_PART_LABEL)
                .map(|unit_id| (name.as_str(), *unit_id))
        })
    }

    pub(crate) fn parts(&self, helmet_name: &str) -> Option<&HashMap<String, u64>> {
        self.helmets.get(helmet_name).map(|mapping| &mapping.parts)
    }

    fn validate(&self) -> crate::Result<()> {
        for (name, mapping) in &self.helmets {
            mapping.validate(name)?;
        }
        Ok(())
    }
}

impl std::str::FromStr for HelmetMappingTable {
    type Err = eyre::Report;

    fn from_str(text: &str) -> std::result::Result<Self, Self::Err> {
        let table: Self = serde_json::from_str(text)?;
        table.validate()?;
        Ok(table)
    }
}

impl HelmetPartMap {
    fn validate(&self, helmet_name: &str) -> crate::Result<()> {
        if self.parts.len() == 1 && self.parts.contains_key(HELMET_PART_LABEL) {
            return Ok(());
        }
        eyre::bail!("helmet mapping {helmet_name:?} must contain only the Helmet Unit ID")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_table_parses() {
        let table = HelmetMappingTable::bundled().unwrap();

        assert!(
            table.helmet_count() > 0,
            "bundled helmet table must not be empty"
        );
        assert_eq!(table.entries().count(), table.helmet_count());
        for (name, unit_id) in table.entries() {
            assert!(!name.trim().is_empty(), "helmet name must not be blank");
            assert_eq!(table.unit_id(name), Some(unit_id));
        }
    }

    #[test]
    fn parses_exact_unit_ids_and_looks_up_by_name() {
        let table: HelmetMappingTable = r#"{
            "Test Helmet A": {"Helmet": 18446744073709551615},
            "Test Helmet B": {"Helmet": 9007199254740993}
        }"#
        .parse()
        .unwrap();

        assert_eq!(table.helmet_count(), 2);
        assert_eq!(table.unit_id("Test Helmet A"), Some(u64::MAX));
        assert_eq!(table.unit_id("Test Helmet B"), Some(9_007_199_254_740_993));
        assert_eq!(table.unit_id("Unknown Helmet"), None);
    }

    #[test]
    fn rejects_missing_or_extra_parts() {
        assert!(r#"{"Bad": {"Body": 1}}"#.parse::<HelmetMappingTable>().is_err());
        assert!(r#"{"Bad": {"Helmet": 1, "Body": 2}}"#.parse::<HelmetMappingTable>().is_err());
    }
}
