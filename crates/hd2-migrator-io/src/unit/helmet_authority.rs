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

        assert_eq!(table.helmet_count(), 107);
        assert_eq!(
            table.unit_id("TG-8 Sharpshooter"),
            Some(16_686_489_699_036_771_610)
        );
        assert_eq!(table.unit_id("UF-84 Doubt Killer"), None);
    }

    #[test]
    fn rejects_missing_or_extra_parts() {
        assert!(r#"{"Bad": {"Body": 1}}"#.parse::<HelmetMappingTable>().is_err());
        assert!(r#"{"Bad": {"Helmet": 1, "Body": 2}}"#.parse::<HelmetMappingTable>().is_err());
    }
}
